//! Item-folder operations — the folder half of every "folder of item files"
//! tier: the wardrobe's `Wardrobe/` and the Inventory's `Inventory/` (v4
//! `lib/database/repositories/vault-overlay/item-folder-ops.ts`, `4fbdad186`;
//! P4.D324 — the LIFT).
//!
//! Listing, creating, renaming and deleting folders is the same work whatever
//! the items are: the path is the truth, empty folders persist as store rows,
//! a rename is a store folder move, and a delete sends everything up a level.
//! What differs per tier — the root, the item predicate, the reader and the
//! projection — is an [`ItemFolderTree`] descriptor, so both tiers call one
//! implementation rather than each keeping a copy. Also here: the one
//! tombstone-aware mount resolver ([`resolve_vault_folder_mount`]).
//!
//! **NO-PORT: `runSerialized` / `chainKey`** (`:86-108`) — v4's per-folder
//! promise-chain map is a Node in-process concurrency guard; v5's writer task
//! serialises every write, and every caller here runs inside one writer
//! closure.
//!
//! The DEBUG lines carry v4's `${rules.root} …` messages; their `context`
//! field is the tree's.

use crate::inventory::lint_scope::TitleLintScope;
use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use super::database_store::{
    delete_database_folder, move_database_folder, DbStoreErrorCode, StoreError,
};
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::doc_mount_folders::DocMountFoldersRepository;
use super::{characters_read, instance_settings, DbError};
use crate::inventory::item_folders::{
    folder_name, folder_segments, folder_under_item_root, is_same_folder, is_under_folder,
    item_folder_store_path, join_folder, parent_folder, rebase_folder, ItemFolder, ItemFolderRules,
};

/// The four tiers a vault folder can belong to (v4 `VaultFolderMountScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultFolderMountScope {
    Character,
    Group,
    Project,
    General,
}

impl VaultFolderMountScope {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            VaultFolderMountScope::Character => "character",
            VaultFolderMountScope::Group => "group",
            VaultFolderMountScope::Project => "project",
            VaultFolderMountScope::General => "general",
        }
    }
}

/// One tier's folder on disk (v4 `VaultFolderMount`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultFolderMount {
    pub mount_point_id: String,
    pub scope: VaultFolderMountScope,
    /// Owning character for the `character` scope; `None` for every shared
    /// tier.
    pub character_id: Option<String>,
}

impl VaultFolderMount {
    /// The parse / log scope the reader and projector take — v4 `characterId
    /// ?? mountPointId`.
    pub fn scope_id(&self) -> &str {
        self.character_id
            .as_deref()
            .unwrap_or(self.mount_point_id.as_str())
    }
}

/// Resolve the vault-folder mount for a character id, or Quilltap General for
/// `None` (v4 `resolveVaultFolderMount`). `Ok(None)` when no mount is
/// available (no linked vault, General not yet provisioned).
/// [`DbError::CharacterArchived`] for an archived character: a pruned vault is
/// still live, so refusing here is what keeps the tombstone. The mount is the
/// same whatever folder is wanted; each tier names its own folder through its
/// rules.
pub fn resolve_vault_folder_mount(
    main: &Connection,
    character_id: Option<&str>,
) -> Result<Option<VaultFolderMount>, DbError> {
    match character_id {
        None => {
            let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
                return Ok(None);
            };
            Ok(Some(VaultFolderMount {
                mount_point_id,
                scope: VaultFolderMountScope::General,
                character_id: None,
            }))
        }
        Some(cid) => {
            let Some(row) = characters_read::find_by_id_raw(main, cid)? else {
                return Ok(None);
            };
            if row
                .get("archivedAt")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            {
                return Err(DbError::CharacterArchived {
                    character_id: cid.to_string(),
                });
            }
            let Some(mount_point_id) = row
                .get("characterDocumentMountPointId")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                return Ok(None);
            };
            Ok(Some(VaultFolderMount {
                mount_point_id: mount_point_id.to_string(),
                scope: VaultFolderMountScope::Character,
                character_id: Some(cid.to_string()),
            }))
        }
    }
}

/// What every filed item carries (v4 `FiledItem`), as the folder ops read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiledItem {
    pub id: String,
    /// `''` for the root.
    pub folder: String,
    pub archived: bool,
}

/// A tier's description of its folder of items (v4 `ItemFolderTree<T>`).
pub trait ItemFolderTree {
    type Item;
    type Error: From<DbError>;

    fn rules(&self) -> &'static ItemFolderRules;
    /// The item files' extension, e.g. `.md`.
    fn ext(&self) -> &'static str;
    /// The `context` field on log lines.
    fn context(&self) -> &'static str;
    /// True for a document this tier owns (and may sweep or move).
    fn is_item_document(&self, relative_path: &str, content: Option<&str>) -> bool;
    /// True when a file carries its own id (moving it keeps its identity).
    fn has_stable_id(&self, content: Option<&str>) -> bool;
    /// The filing facts of one item.
    fn filed(&self, item: &Self::Item) -> FiledItem;
    /// Every item in the folder, archived included.
    fn read(&self, mount: &Connection, m: &VaultFolderMount) -> Result<Vec<Self::Item>, DbError>;
    /// Re-project the folder from an authoritative list.
    fn reproject(
        &self,
        mount: &Connection,
        m: &VaultFolderMount,
        items: &[Self::Item],
    ) -> Result<(), DbError>;
    /// Re-file items with one projection (`current` is the folder as the
    /// delete already read it; a tier whose v4 `refile` re-reads may ignore
    /// it).
    fn refile(
        &self,
        main: &Connection,
        mount: &Connection,
        m: &VaultFolderMount,
        current: &[Self::Item],
        patches: &[(String, String)],
    ) -> Result<(), Self::Error>;
    /// v4 `errors.path(message)`.
    fn path_error(&self, message: String) -> Self::Error;
    /// v4 `errors.notFound(folder)`.
    fn not_found(&self, folder: &str) -> Self::Error;
    /// v4 `errors.conflict(folder, message?)`.
    fn conflict(&self, folder: &str, message: Option<String>) -> Self::Error;
}

/// The pure half of [`list_folders_in`]: the store rows' folders, then the
/// item-implied ones, every ancestor included, the first spelling per
/// lower-cased key winning; counted, then sorted by path at BASE strength.
/// v4 builds an insertion-ordered `Map` (rows first, then items) and V8's
/// sort is stable, so two paths that compare EQUAL at base strength yet
/// differ in key keep that insertion order.
pub(crate) fn assemble_folders(
    row_folders: impl Iterator<Item = String>,
    items: &[FiledItem],
) -> Vec<ItemFolder> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut by_key: Vec<(String, String)> = Vec::new();
    let mut add = |folder: &str| {
        // Every ancestor is a folder too, even when only a deep path has a row.
        let segments = folder_segments(folder);
        for depth in 1..=segments.len() {
            let path = segments[..depth].join("/");
            let key = path.to_lowercase();
            if seen.insert(key.clone()) {
                by_key.push((key, path));
            }
        }
    };
    for folder in row_folders {
        add(&folder);
    }
    for item in items {
        if !item.folder.is_empty() {
            add(&item.folder);
        }
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for item in items {
        if item.archived || item.folder.is_empty() {
            continue;
        }
        *counts.entry(item.folder.to_lowercase()).or_insert(0) += 1;
    }
    let mut folders: Vec<ItemFolder> = by_key
        .into_iter()
        .map(|(key, path)| ItemFolder {
            name: folder_name(&path).to_string(),
            item_count: counts.get(&key).copied().unwrap_or(0),
            path,
        })
        .collect();
    folders.sort_by(|a, b| crate::collation::locale_compare_base(&a.path, &b.path));
    folders
}

/// Every folder under the tier's root (v4 `listFoldersIn`): the union of the
/// store's folder rows (empty folders persist as rows) and the folders implied
/// by item paths, minus reserved ones. `itemCount` counts non-archived items
/// directly inside. Sorted by path, case-insensitively. DEBUG `${root}
/// folders listed`.
pub fn list_folders_in<T: ItemFolderTree>(
    tree: &T,
    mount: &Connection,
    m: &VaultFolderMount,
) -> Result<Vec<ItemFolder>, DbError> {
    let rows =
        DocMountFoldersRepository::new(mount).find_by_mount_point_id_or_empty(&m.mount_point_id);
    let items: Vec<FiledItem> = tree.read(mount, m)?.iter().map(|i| tree.filed(i)).collect();
    let folders = assemble_folders(
        rows.iter()
            .filter_map(|row| folder_under_item_root(tree.rules(), &row.path)),
        &items,
    );
    tracing::debug!(
        mountPointId = %m.mount_point_id,
        scope = m.scope.as_str(),
        folderRowCount = rows.len(),
        folderCount = folders.len(),
        context = tree.context(),
        "{} folders listed",
        tree.rules().root
    );
    Ok(folders)
}

/// The paths of every folder under the tier's root, from the store's folder
/// rows alone, minus reserved ones (v4 `listFolderPathsIn`). One query, no
/// item parse. A fallback read: never fails.
pub fn list_folder_paths_in<T: ItemFolderTree>(
    tree: &T,
    mount: &Connection,
    m: &VaultFolderMount,
) -> Vec<String> {
    DocMountFoldersRepository::new(mount)
        .find_by_mount_point_id_or_empty(&m.mount_point_id)
        .iter()
        .filter_map(|row| folder_under_item_root(tree.rules(), &row.path))
        .collect()
}

/// The stored casing of a folder that exists, or `None` (v4
/// `findStoredFolder` — the store's path lookup is case-insensitive, a
/// fallback read).
pub fn find_stored_folder(
    rules: &ItemFolderRules,
    mount: &Connection,
    mount_point_id: &str,
    folder: &str,
) -> Option<String> {
    DocMountFoldersRepository::new(mount)
        .find_by_mount_point_and_path_or_none(
            mount_point_id,
            &item_folder_store_path(rules, folder),
        )
        .and_then(|row| folder_under_item_root(rules, &row.path))
}

/// Create a folder (and its missing ancestors) (v4 `createFolderIn`).
/// Idempotent and case-insensitive: creating `formal` beside `Formal` answers
/// `Formal`. `folder` must already be normalised; `''` is refused (`The
/// ${noun} root already exists`). DEBUG `${root} folder ensured`.
pub fn create_folder_in<T: ItemFolderTree>(
    tree: &T,
    mount: &Connection,
    m: &VaultFolderMount,
    folder: &str,
) -> Result<ItemFolder, T::Error> {
    let rules = tree.rules();
    if folder.is_empty() {
        return Err(tree.path_error(format!("The {} root already exists", rules.noun)));
    }
    DocMountFileLinksRepository::new(mount)
        .ensure_folder_path(&m.mount_point_id, &item_folder_store_path(rules, folder))?;
    let stored = find_stored_folder(rules, mount, &m.mount_point_id, folder)
        .unwrap_or_else(|| folder.to_string());
    let items = tree.read(mount, m)?;
    let item_count = items
        .iter()
        .map(|i| tree.filed(i))
        .filter(|i| !i.archived && is_same_folder(Some(&i.folder), Some(&stored)))
        .count();
    tracing::debug!(
        mountPointId = %m.mount_point_id,
        scope = m.scope.as_str(),
        requested = %folder,
        folder = %stored,
        itemCount = item_count,
        context = tree.context(),
        "{} folder ensured",
        rules.root
    );
    Ok(ItemFolder {
        name: folder_name(&stored).to_string(),
        path: stored,
        item_count,
    })
}

/// Map a store-level failure of a folder move / delete onto the tree's errors
/// (v4's `DatabaseStoreError` code switches). A code the caller does not map
/// is v4's `throw error` — the store's own sentence as-is.
fn store_error<T: ItemFolderTree>(
    e: StoreError,
    conflict: Option<&dyn Fn() -> T::Error>,
    not_found: Option<&dyn Fn() -> T::Error>,
    not_empty: Option<&dyn Fn() -> T::Error>,
) -> T::Error {
    match e {
        StoreError::Store(s) => {
            let mapped = match s.code {
                DbStoreErrorCode::Conflict => conflict,
                DbStoreErrorCode::NotFound => not_found,
                DbStoreErrorCode::NotEmpty => not_empty,
                _ => None,
            };
            match mapped {
                Some(f) => f(),
                None => DbError::Internal(s.message).into(),
            }
        }
        StoreError::Db(d) => d.into(),
        StoreError::TitleLint(r) => DbError::Internal(r.message).into(),
    }
}

/// Rename a folder in place (`new_name` is one normalised segment) (v4
/// `renameFolderIn`). Its items, subfolders and any other files move with it —
/// the path is the truth. A sibling already holding the name (in any casing)
/// is a conflict; a case-only rename is allowed. The store moves the files
/// without rewriting them, and an item file without its own id takes its id
/// from its path — moving it would mint a new one and orphan everything that
/// names it — so every such file is STAMPED with the id it has now, first
/// (one projection of the whole tier; DEBUG `Stamping ids into ${noun} files
/// before a folder rename`). DEBUG `${root} folder renamed`.
pub fn rename_folder_in<T: ItemFolderTree>(
    tree: &T,
    mount: &Connection,
    m: &VaultFolderMount,
    folder: &str,
    new_name: &str,
    linted: TitleLintScope<'_>,
) -> Result<ItemFolder, T::Error> {
    let rules = tree.rules();
    if folder.is_empty() {
        return Err(tree.path_error(format!("The {} root cannot be renamed", rules.noun)));
    }
    let Some(from) = find_stored_folder(rules, mount, &m.mount_point_id, folder) else {
        return Err(tree.not_found(folder));
    };
    let to = join_folder(&[parent_folder(&from), new_name]);
    if to != from {
        let docs_in = DocMountDocumentsRepository::new(mount)
            .find_many_by_mount_points_in_folder_opts_or_empty(
                std::slice::from_ref(&m.mount_point_id),
                &item_folder_store_path(rules, &from),
                tree.ext(),
                true,
            )?;
        let unstamped = docs_in
            .iter()
            .filter(|d| {
                tree.is_item_document(&d.relative_path, Some(&d.content))
                    && !tree.has_stable_id(Some(&d.content))
            })
            .count();
        if unstamped > 0 {
            tracing::debug!(
                mountPointId = %m.mount_point_id,
                folder = %from,
                count = unstamped,
                context = tree.context(),
                "Stamping ids into {} files before a folder rename",
                rules.noun
            );
            let items = tree.read(mount, m)?;
            tree.reproject(mount, m, &items)?;
        }
        move_database_folder(
            mount,
            &m.mount_point_id,
            &item_folder_store_path(rules, &from),
            &item_folder_store_path(rules, &to),
            linted,
        )
        .map_err(|e| {
            store_error::<T>(
                e,
                Some(&|| tree.conflict(&to, None)),
                Some(&|| tree.not_found(folder)),
                None,
            )
        })?;
    }
    let items = tree.read(mount, m)?;
    let item_count = items
        .iter()
        .map(|i| tree.filed(i))
        .filter(|i| !i.archived && is_same_folder(Some(&i.folder), Some(&to)))
        .count();
    tracing::debug!(
        mountPointId = %m.mount_point_id,
        scope = m.scope.as_str(),
        from = %from,
        to = %to,
        itemCount = item_count,
        context = tree.context(),
        "{} folder renamed",
        rules.root
    );
    Ok(ItemFolder {
        name: folder_name(&to).to_string(),
        path: to,
        item_count,
    })
}

/// What [`delete_folder_in`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedFolder {
    pub moved_items: usize,
    pub moved_folders: usize,
    pub parent: String,
}

/// Delete a folder, sending everything in it up one level (v4
/// `deleteFolderIn`): its items (archived included) to the parent, its
/// subfolders to sit beside it — merging into a same-named folder there,
/// case-insensitively. Nothing is deleted but folder rows. Everything is
/// checked BEFORE anything moves: (1) files the tier doesn't own refuse with
/// `Folder "<f>" holds N file(s) that are not ${noun} items; move them in the
/// Scriptorium first`; (2) a subfolder sharing the folder's own name refuses
/// with `Folder "<f>" holds a subfolder of the same name; rename one of them
/// first`. Items go in ONE refile, each re-rooted; subfolders are re-created
/// under the parent, then the old rows dropped deepest first; a late
/// `NOT_EMPTY` is `Folder "<f>" is not empty yet; its items were moved up`.
/// DEBUG `${root} folder deleted; contents sent up`.
pub fn delete_folder_in<T: ItemFolderTree>(
    tree: &T,
    main: &Connection,
    mount: &Connection,
    m: &VaultFolderMount,
    folder: &str,
) -> Result<DeletedFolder, T::Error> {
    let rules = tree.rules();
    if folder.is_empty() {
        return Err(tree.path_error(format!("The {} root cannot be deleted", rules.noun)));
    }
    let Some(from) = find_stored_folder(rules, mount, &m.mount_point_id, folder) else {
        return Err(tree.not_found(folder));
    };
    let parent = parent_folder(&from).to_string();
    let store_from = item_folder_store_path(rules, &from);
    let store_prefix = format!("{store_from}/").to_lowercase();

    let links_repo = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    let folders = DocMountFoldersRepository::new(mount);

    // 1. Files the tier doesn't own.
    let links: Vec<_> = links_repo
        .find_by_mount_point_id_or_empty(&m.mount_point_id)
        .into_iter()
        .filter(|l| l.relative_path.to_lowercase().starts_with(&store_prefix))
        .collect();
    let docs_in = docs.find_many_by_mount_points_in_folder_opts_or_empty(
        std::slice::from_ref(&m.mount_point_id),
        &store_from,
        tree.ext(),
        true,
    )?;
    let docs_by_path: HashMap<String, &str> = docs_in
        .iter()
        .map(|d| (d.relative_path.to_lowercase(), d.content.as_str()))
        .collect();
    let strays = links
        .iter()
        .filter(
            |l| match docs_by_path.get(&l.relative_path.to_lowercase()) {
                None => true,
                Some(content) => !tree.is_item_document(&l.relative_path, Some(content)),
            },
        )
        .count();
    if strays > 0 {
        return Err(tree.conflict(
            &from,
            Some(format!(
                "Folder \"{from}\" holds {strays} file(s) that are not {} items; move them in the Scriptorium first",
                rules.noun
            )),
        ));
    }

    // 2. A subfolder sharing the folder's own name.
    let mut rows: Vec<_> = folders
        .find_by_mount_point_id_or_empty(&m.mount_point_id)
        .into_iter()
        .filter(|r| r.path.to_lowercase().starts_with(&store_prefix))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.path.encode_utf16().count()));
    let items = tree.read(mount, m)?;
    let moving: Vec<FiledItem> = items
        .iter()
        .map(|i| tree.filed(i))
        .filter(|item| is_under_folder(&item.folder, &from))
        .collect();
    let lands_inside = moving
        .iter()
        .map(|item| item.folder.clone())
        .chain(
            rows.iter()
                .map(|row| folder_under_item_root(rules, &row.path).unwrap_or_default()),
        )
        .any(|sub| !sub.is_empty() && is_under_folder(&rebase_folder(&sub, &from, &parent), &from));
    if lands_inside {
        return Err(tree.conflict(
            &from,
            Some(format!(
                "Folder \"{from}\" holds a subfolder of the same name; rename one of them first"
            )),
        ));
    }

    // Items: one batch, one projection.
    let moved_items = moving.len();
    if moved_items > 0 {
        let patches: Vec<(String, String)> = moving
            .iter()
            .map(|item| (item.id.clone(), rebase_folder(&item.folder, &from, &parent)))
            .collect();
        tree.refile(main, mount, m, &items, &patches)?;
    }

    // Subfolders: re-create each (empty ones included) under the parent, then
    // drop the old rows deepest first.
    for row in &rows {
        let Some(sub) = folder_under_item_root(rules, &row.path) else {
            continue;
        };
        links_repo.ensure_folder_path(
            &m.mount_point_id,
            &item_folder_store_path(rules, &rebase_folder(&sub, &from, &parent)),
        )?;
    }
    let not_empty = || {
        tree.conflict(
            &from,
            Some(format!(
                "Folder \"{from}\" is not empty yet; its items were moved up"
            )),
        )
    };
    for row in &rows {
        delete_database_folder(mount, &m.mount_point_id, &row.path)
            .map_err(|e| store_error::<T>(e, None, None, Some(&not_empty)))?;
    }
    delete_database_folder(mount, &m.mount_point_id, &store_from)
        .map_err(|e| store_error::<T>(e, None, None, Some(&not_empty)))?;

    let child_depth = folder_segments(&from).len() + 1;
    let moved_folders = rows
        .iter()
        .filter(|row| {
            folder_segments(&folder_under_item_root(rules, &row.path).unwrap_or_default()).len()
                == child_depth
        })
        .count();
    tracing::debug!(
        mountPointId = %m.mount_point_id,
        scope = m.scope.as_str(),
        folder = %from,
        parent = %parent,
        movedItems = moved_items,
        movedFolders = moved_folders,
        context = tree.context(),
        "{} folder deleted; contents sent up",
        rules.root
    );
    Ok(DeletedFolder {
        moved_items,
        moved_folders,
        parent,
    })
}
