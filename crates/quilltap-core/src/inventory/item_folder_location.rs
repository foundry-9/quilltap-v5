//! The folder half every tiered item location shares, and the folder
//! mutations both kinds file through (v4 `lib/mount-index/item-folder-location.ts`,
//! `a0ed5ecad`; P4.D377 item 2).
//!
//! Kind-agnostic despite the directory: it sits beside
//! [`super::item_folders`] (v4's `item-folders.ts`, which both kinds already
//! share) — the wardrobe's [`WardrobeLocation`](crate::db::wardrobe_location::WardrobeLocation)
//! and the Inventory's [`InventoryLocation`](super::location::InventoryLocation)
//! both implement [`ItemFolderLocation`] / [`ItemFilingLocation`].
//!
//! Folders are the operator's filing. An item's folder is its path under the
//! kind's root and nothing else: a rename moves the files, a delete sends them
//! up a level, and nothing here ever deletes an item.
//!
//! **One log shape.** v4 `logFolderOp` emits every folder mutation as ONE
//! DEBUG `[ItemFolders] ${op}` with the keys `kind` (= `rules.noun`), `scope`,
//! `mountPointId`, the op's own keys, `context` (= `rules.noun`) — replacing
//! the wardrobe's `[FolderMutations] … wardrobe folder` / `[ItemMutations]
//! Moved wardrobe items to folder` and the Inventory's `[InventoryMutations]`
//! folder lines (which lose their `op` key and gain `itemCount` on create /
//! rename — v4's "no behaviour change" claim measured FALSE, P4.D377 §R.4).
//! Tracing field names are static, so the one log helper has one arm per op.
//!
//! **Shape (recorded, the order's item 2):** the kind is a struct of its rules
//! plus a path-error constructor generic over the location's error type
//! (`ItemFolderKind<E>`); connections are the explicit `main` / `mount`
//! arguments both locations already take (v4's closures over the repository
//! container). The DEBUGs run INSIDE the caller's `Db::write` closure — on the
//! writer thread, so their pins use `global_capture` (memory note
//! `capture-rig-writer-thread`).

use rusqlite::Connection;

use super::item_folders::{ItemFolder, ItemFolderRules, EMPTY_NAME_MESSAGE};
use crate::db::item_folder_ops::DeletedFolder;
use crate::jsstr::js_trim;

/// One tier's folder tree under the kind's root (v4 `ItemFolderLocation`).
pub trait ItemFolderLocation {
    /// The location's error class (v4's thrown errors).
    type Error;

    /// The tier's wire name — `character` / `group` / `project` / `general`.
    fn scope(&self) -> &'static str;
    /// The mount holding the root folder.
    fn mount_point_id(&self) -> &str;
    /// Every folder under the root (empty ones included), with direct item
    /// counts. A READ — no tombstone.
    fn list_folders(&self, mount: &Connection) -> Result<Vec<ItemFolder>, crate::db::DbError>;
    /// An operator's requested folder, resolved against this tier's folders
    /// (an existing one as stored, else the new part normalised; blank → `''`).
    fn resolve_folder(&self, mount: &Connection, requested: &str) -> Result<String, Self::Error>;
    fn create_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<ItemFolder, Self::Error>;
    fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<ItemFolder, Self::Error>;
    fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, Self::Error>;
}

/// A location that can re-file its own items (the batch `move-items`) — v4
/// `ItemFilingLocation<T>`'s `updateMany(patches, { requireAll })`, narrowed to
/// the `{ folder }` patch it is called with.
pub trait ItemFilingLocation: ItemFolderLocation {
    type Item;
    /// Patch every id's folder to `folder` in ONE projection; an id not in this
    /// location refuses the batch (`requireAll: true`).
    fn refile_items(
        &self,
        main: &Connection,
        mount: &Connection,
        item_ids: &[String],
        folder: &str,
    ) -> Result<Vec<Self::Item>, Self::Error>;
}

/// What tells one kind's folders from another's: its rules and its path error
/// (v4 `ItemFolderKind`).
pub struct ItemFolderKind<E> {
    pub rules: &'static ItemFolderRules,
    pub path_error: fn(String) -> E,
}

/// v4 `locationKey(loc)` — `${scope}:${mountPointId}`, a key that names one
/// folder tree, for "same source and destination?" checks.
pub fn location_key<L: ItemFolderLocation + ?Sized>(loc: &L) -> String {
    format!("{}:{}", loc.scope(), loc.mount_point_id())
}

/// v4 `trimFolderPath`: `path.trim().replace(/^\/+|\/+$/g, '')` — the folder
/// named, matched as stored (hand-made spellings included), only the edges
/// tidied.
fn trim_folder_path(path: &str) -> String {
    js_trim(path).trim_matches('/').to_string()
}

/// The op a [`log_folder_op`] line names, with that op's own keys.
enum FolderOp<'a> {
    Created {
        requested: &'a str,
        folder: &'a str,
        item_count: usize,
    },
    Renamed {
        from: &'a str,
        to: &'a str,
        item_count: usize,
    },
    Deleted {
        folder: &'a str,
        parent: &'a str,
        moved_items: usize,
        moved_folders: usize,
    },
    Moved {
        folder: &'a str,
        requested: usize,
        moved: usize,
    },
}

/// v4 `logFolderOp(op, kind, location, meta)` — ONE DEBUG `[ItemFolders]
/// ${op}`, keys `kind, scope, mountPointId, …meta, context`.
fn log_folder_op<L: ItemFolderLocation + ?Sized>(
    rules: &ItemFolderRules,
    location: &L,
    op: FolderOp<'_>,
) {
    let (kind, scope, mount_point_id) = (rules.noun, location.scope(), location.mount_point_id());
    match op {
        FolderOp::Created {
            requested,
            folder,
            item_count,
        } => tracing::debug!(
            kind,
            scope,
            mountPointId = %mount_point_id,
            requested = %requested,
            folder = %folder,
            itemCount = item_count,
            context = kind,
            "[ItemFolders] Created folder"
        ),
        FolderOp::Renamed {
            from,
            to,
            item_count,
        } => tracing::debug!(
            kind,
            scope,
            mountPointId = %mount_point_id,
            from = %from,
            to = %to,
            itemCount = item_count,
            context = kind,
            "[ItemFolders] Renamed folder"
        ),
        FolderOp::Deleted {
            folder,
            parent,
            moved_items,
            moved_folders,
        } => tracing::debug!(
            kind,
            scope,
            mountPointId = %mount_point_id,
            folder = %folder,
            parent = %parent,
            movedItems = moved_items,
            movedFolders = moved_folders,
            context = kind,
            "[ItemFolders] Deleted folder"
        ),
        FolderOp::Moved {
            folder,
            requested,
            moved,
        } => tracing::debug!(
            kind,
            scope,
            mountPointId = %mount_point_id,
            folder = %folder,
            requested,
            moved,
            context = kind,
            "[ItemFolders] Moved items to folder"
        ),
    }
}

/// v4 `createItemFolder(kind, location, path)` — create a folder at a location
/// (an existing one, in any casing, is answered). A path that resolves to the
/// root refuses with the kind's path error `A folder needs a name` and logs
/// nothing.
pub fn create_item_folder<L: ItemFolderLocation + ?Sized>(
    kind: &ItemFolderKind<L::Error>,
    main: &Connection,
    mount: &Connection,
    location: &L,
    path: &str,
) -> Result<ItemFolder, L::Error> {
    let resolved = location.resolve_folder(mount, path)?;
    if resolved.is_empty() {
        return Err((kind.path_error)(EMPTY_NAME_MESSAGE.to_string()));
    }
    let folder = location.create_folder(main, mount, &resolved)?;
    log_folder_op(
        kind.rules,
        location,
        FolderOp::Created {
            requested: path,
            folder: &folder.path,
            item_count: folder.item_count,
        },
    );
    Ok(folder)
}

/// v4 `renameItemFolder(kind, location, path, name)` — rename a folder in
/// place; its items follow because the path is the truth. The folder named is
/// an existing one, matched as stored; the store answers a missing one with a
/// 404. The new name is one segment through the kind's rules
/// (`normalizeItemFolderName`).
pub fn rename_item_folder<L: ItemFolderLocation + ?Sized>(
    kind: &ItemFolderKind<L::Error>,
    main: &Connection,
    mount: &Connection,
    location: &L,
    path: &str,
    name: &str,
) -> Result<ItemFolder, L::Error> {
    let from = trim_folder_path(path);
    let new_name = super::item_folders::normalize_item_folder_name(kind.rules, Some(name))
        .map_err(kind.path_error)?;
    let folder = location.rename_folder(main, mount, &from, &new_name)?;
    log_folder_op(
        kind.rules,
        location,
        FolderOp::Renamed {
            from: &from,
            to: &folder.path,
            item_count: folder.item_count,
        },
    );
    Ok(folder)
}

/// v4 `deleteItemFolder`'s `{ movedItems, movedFolders }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeletedItemFolder {
    pub moved_items: usize,
    pub moved_folders: usize,
}

/// v4 `deleteItemFolder(kind, location, path)` — delete a folder, sending its
/// items and subfolders up to its parent.
pub fn delete_item_folder<L: ItemFolderLocation + ?Sized>(
    kind: &ItemFolderKind<L::Error>,
    main: &Connection,
    mount: &Connection,
    location: &L,
    path: &str,
) -> Result<DeletedItemFolder, L::Error> {
    let folder = trim_folder_path(path);
    let deleted = location.delete_folder(main, mount, &folder)?;
    log_folder_op(
        kind.rules,
        location,
        FolderOp::Deleted {
            folder: &folder,
            parent: &deleted.parent,
            moved_items: deleted.moved_items,
            moved_folders: deleted.moved_folders,
        },
    );
    Ok(DeletedItemFolder {
        moved_items: deleted.moved_items,
        moved_folders: deleted.moved_folders,
    })
}

/// `Array.from(new Set(ids))` — first occurrence kept, order kept.
fn unique_in_order(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// v4 `moveItemsToFolderIn(kind, location, itemIds, folder)` — file several
/// items into one folder at a location, with one read and one projection.
/// Every id must be an item of this location — a borrowed item (one shown in a
/// character's view from a shared tier) refuses the batch, naming it, before
/// anything moves. Returns the moved items as written.
pub fn move_items_to_folder_in<L: ItemFilingLocation + ?Sized>(
    kind: &ItemFolderKind<L::Error>,
    main: &Connection,
    mount: &Connection,
    location: &L,
    item_ids: &[String],
    folder: &str,
) -> Result<Vec<L::Item>, L::Error> {
    let target = location.resolve_folder(mount, folder)?;
    let unique_ids = unique_in_order(item_ids);
    let moved = location.refile_items(main, mount, &unique_ids, &target)?;
    log_folder_op(
        kind.rules,
        location,
        FolderOp::Moved {
            folder: &target,
            requested: unique_ids.len(),
            moved: moved.len(),
        },
    );
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_folder_path_tidies_only_the_edges() {
        assert_eq!(trim_folder_path("  /Formal/Evening//  "), "Formal/Evening");
        assert_eq!(trim_folder_path("Hats: Summer"), "Hats: Summer");
        assert_eq!(trim_folder_path("///"), "");
        // JS trim: U+00A0 counts as whitespace.
        assert_eq!(trim_folder_path("\u{00a0}/Casual"), "Casual");
    }

    #[test]
    fn unique_keeps_first_occurrence_order() {
        let ids: Vec<String> = ["b", "a", "b", "c", "a"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(unique_in_order(&ids), vec!["b", "a", "c"]);
    }
}
