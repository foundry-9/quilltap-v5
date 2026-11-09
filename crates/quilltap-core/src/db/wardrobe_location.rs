//! Wardrobe Location — the one answer to "where does this item live, and how
//! do I read and write there" (v4 `lib/wardrobe/location.ts`, `9ecebf986` +
//! `4a38222e6`; P4.D279, keystone C1 §2).
//!
//! A location is one wardrobe tier's `Wardrobe/` folder: a character's vault,
//! Quilltap General, a project's official store or a group's official store.
//! It always carries the backing mount, the origin a collection read tags its
//! items with, the item operations every caller needs, and the folder
//! operations the browser files with (an item's folder is its path under
//! `Wardrobe/` and nothing else). Routes, transfers, item pictures and the
//! tools all address a tier through this; nobody switches on scope to pick a
//! writer, and nobody passes an owner-id hint.
//!
//! **Provisioning is opt-in (R-C):** `ensure: true` creates a project/group
//! store and its `Wardrobe/` folder when missing, and is for callers about to
//! write. A read probe passes `false` and gets `None` for a store that doesn't
//! exist yet — so a GET of an unprovisioned project's wardrobe answers an
//! empty list WITHOUT writing (v4's old container resolver provisioned on
//! every resolve; the pin's does not).
//!
//! **The ownership gate (R-E)** lives here: a character that is not `user_id`'s
//! resolves to `None`.
//!
//! The archived-character tombstone stays where it lives
//! ([`resolve_wardrobe_mount`] answers [`DbError::CharacterArchived`]): an
//! archived character's location still READS, but every write through it —
//! and [`WardrobeLocation::writable_mount_point_id`], for writers that touch
//! the folder some other way — refuses with [`WardrobeLocationError::CharacterArchived`].
//! Writes re-resolve a character's mount AT WRITE TIME so the tombstone check
//! sees the character as it is now.
//!
//! **Every write tells the open browsers (v4 `600c2906f`, P4.D377 item 8).**
//! One private [`WardrobeLocation::write`] (v4's `write(op, fn, landed)`) sits
//! under every write method: it runs the write against the writable mount,
//! publishes the collection-wide `wardrobe` realtime topic only when something
//! LANDED (a batch or delete that matched nothing changed nothing), and logs
//! the DEBUG `[WardrobeLocation] Location write {op, mountPointId, scope,
//! published, context}` on every write that returned — published or not. A
//! write that throws (the tombstone, a refusal) logs nothing and publishes
//! nothing. The publish is a synchronous, coalescing, fire-and-forget call,
//! safe inside a write closure (the Inventory's `landed()` precedent); in v4's
//! job child the publish is a no-op and the commit hook names the topic
//! instead (`writesWardrobeFile` in `realtime::job_topics`).
//!
//! **Owners are read RAW (v4 `resolveOwnerStore`, `0c47f4539`; P4.D377 item
//! 9, the survey's R-F):** a project / group whose row fails hydration still
//! resolves its store; the folder heal on an ensuring resolve is reach's, and
//! its failure is reach's `[Reach] Failed to ensure Wardrobe folder` WARN.

use rusqlite::Connection;

use super::archetype_wardrobe::{general_wardrobe_origin, WardrobeOrigin, WardrobeOriginScope};
use super::reach::{
    group_mounts_for, group_origin_map, resolve_owner_store, Archived, EnsureFolder,
    OwnerFolderFailure, OwnerScope, ResolveOwnerStoreOptions,
};
use super::tiered_mount_pool::GroupMountsOptions;
use super::vault_wardrobe_public::{
    create_folder_in_mount, create_in_mount, delete_folder_in_mount, delete_in_mount,
    list_folder_paths_in_mount, list_folders_in_mount, read_mount_items, rename_folder_in_mount,
    resolve_wardrobe_mount, update_many_in_mount, DeletedFolder, UpdateManyOptions,
    WardrobeItemPatch, WardrobePatch, WardrobePublicError,
};
pub use super::vault_wardrobe_public::{WardrobeMount, WardrobeScope};
use super::DbError;
use crate::inventory::item_folder_location::{ItemFilingLocation, ItemFolderLocation};
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe_folders::{
    conflict_default_message, not_found_message, not_in_location_message, resolve_wardrobe_folder,
    WardrobeFolder, WardrobeFolderPathError,
};

/// A failure on a location's write path — v4's thrown error classes, each with
/// its sentence ([`Self::message`]). The route factory's write-error map is
/// built on these variants (cycle → 400, path → 400, not-in-location → 400,
/// folder-not-found → 404, conflict → 409, archived character → 409).
#[derive(Debug)]
pub enum WardrobeLocationError {
    /// v4 `CharacterArchivedError` — the tombstone.
    CharacterArchived { character_id: String },
    /// v4 `` throw new Error(`Character ${id} has no linked vault`) `` from the
    /// location's write-time mount re-resolve.
    NoLinkedVault { character_id: String },
    /// v4 `WardrobeComponentCycleError` — carries v4's exact message.
    Cycle(String),
    /// v4 `WardrobeFolderPathError`.
    FolderPath(WardrobeFolderPathError),
    /// v4 `WardrobeFolderNotFoundError`.
    FolderNotFound { folder: String },
    /// v4 `WardrobeFolderConflictError` — the default or a specific message.
    FolderConflict { folder: String, message: String },
    /// v4 `WardrobeItemNotInLocationError`.
    ItemNotInLocation { item_ids: Vec<String> },
    /// An underlying storage error.
    Db(DbError),
}

impl WardrobeLocationError {
    /// v4's `error.message`, byte for byte.
    pub fn message(&self) -> String {
        match self {
            WardrobeLocationError::CharacterArchived { character_id } => {
                DbError::character_archived_message(character_id)
            }
            WardrobeLocationError::NoLinkedVault { character_id } => {
                format!("Character {character_id} has no linked vault")
            }
            WardrobeLocationError::Cycle(m) => m.clone(),
            WardrobeLocationError::FolderPath(e) => e.0.clone(),
            WardrobeLocationError::FolderNotFound { folder } => not_found_message(folder),
            WardrobeLocationError::FolderConflict { message, .. } => message.clone(),
            WardrobeLocationError::ItemNotInLocation { item_ids } => {
                not_in_location_message(item_ids)
            }
            WardrobeLocationError::Db(e) => crate::db::fallback::error_text(e),
        }
    }

    /// The default-message conflict (v4 `new WardrobeFolderConflictError(folder)`).
    pub fn folder_conflict(folder: &str) -> Self {
        WardrobeLocationError::FolderConflict {
            folder: folder.to_string(),
            message: conflict_default_message(folder),
        }
    }
}

impl std::fmt::Display for WardrobeLocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for WardrobeLocationError {}

impl From<DbError> for WardrobeLocationError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::CharacterArchived { character_id } => {
                WardrobeLocationError::CharacterArchived { character_id }
            }
            other => WardrobeLocationError::Db(other),
        }
    }
}

impl From<WardrobeFolderPathError> for WardrobeLocationError {
    fn from(e: WardrobeFolderPathError) -> Self {
        WardrobeLocationError::FolderPath(e)
    }
}

/// The pre-`9ecebf986` public write error, for the kept owner-hint adapters
/// (`create_vault_wardrobe_item` & co.) whose callers still match on it.
impl From<WardrobeLocationError> for WardrobePublicError {
    fn from(e: WardrobeLocationError) -> Self {
        match e {
            WardrobeLocationError::Cycle(m) => WardrobePublicError::Cycle(m),
            WardrobeLocationError::Db(d) => WardrobePublicError::Db(d),
            WardrobeLocationError::CharacterArchived { character_id } => {
                WardrobePublicError::Db(DbError::CharacterArchived { character_id })
            }
            WardrobeLocationError::NoLinkedVault { .. } => WardrobePublicError::NoMount,
            other => WardrobePublicError::Db(DbError::Internal(other.message())),
        }
    }
}

/// v4 `ResolveWardrobeLocationOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolveWardrobeLocationOptions {
    /// Provision a missing project/group store and its folder. For writers.
    pub ensure: bool,
}

/// v4 `WardrobeLocation` — one tier's `Wardrobe/` folder with its origin.
#[derive(Debug, Clone)]
pub struct WardrobeLocation {
    pub scope: WardrobeScope,
    /// Owner id; `None` for General.
    pub id: Option<String>,
    /// The mount holding the `Wardrobe/` folder — always set, every tier.
    pub mount_point_id: String,
    /// The owning character — `character` scope only.
    pub character_id: Option<String>,
    /// What a collection read tags this tier's items with.
    pub origin: WardrobeOrigin,
    mount: WardrobeMount,
}

/// v4 `locationKey(loc)` — a key that names one folder, for "same source and
/// destination?" checks.
pub fn location_key(scope: WardrobeScope, mount_point_id: &str) -> String {
    format!("{}:{mount_point_id}", scope.as_str())
}

/// v4 `locationForMount(mount, id, origin)` — build a location over a folder.
pub fn location_for_mount(
    mount: WardrobeMount,
    id: Option<&str>,
    origin: WardrobeOrigin,
) -> WardrobeLocation {
    WardrobeLocation {
        scope: mount.scope,
        id: id.map(str::to_string),
        mount_point_id: mount.mount_point_id.clone(),
        character_id: mount.character_id.clone(),
        origin,
        mount,
    }
}

impl WardrobeLocation {
    /// The backing mount as resolved (a READ mount — no tombstone check).
    pub fn mount(&self) -> &WardrobeMount {
        &self.mount
    }

    /// `locationKey(this)`.
    pub fn key(&self) -> String {
        location_key(self.scope, &self.mount_point_id)
    }

    /// v4's `writableMount()`: a non-character mount as given; a character's
    /// mount RE-RESOLVED at write time (the tombstone → `CharacterArchived`; a
    /// vanished vault → `NoLinkedVault`).
    fn writable_mount(&self, main: &Connection) -> Result<WardrobeMount, WardrobeLocationError> {
        if self.mount.scope != WardrobeScope::Character {
            return Ok(self.mount.clone());
        }
        let character_id = self.mount.character_id.clone().unwrap_or_default();
        match resolve_wardrobe_mount(main, Some(&character_id))? {
            Some(current) => Ok(current),
            None => Err(WardrobeLocationError::NoLinkedVault { character_id }),
        }
    }

    /// Every item in the folder; archived ones only when asked.
    pub fn read_items(
        &self,
        mount: &Connection,
        include_archived: bool,
    ) -> Result<Vec<WardrobeItem>, DbError> {
        let items = read_mount_items(mount, &self.mount)?;
        Ok(if include_archived {
            items
        } else {
            items
                .into_iter()
                .filter(|item| !matches!(&item.archived_at, Some(Some(a)) if !a.is_empty()))
                .collect()
        })
    }

    /// One item from the folder (archived included), or `None`.
    pub fn find_item(&self, mount: &Connection, id: &str) -> Result<Option<WardrobeItem>, DbError> {
        Ok(read_mount_items(mount, &self.mount)?
            .into_iter()
            .find(|item| item.id == id))
    }

    /// v4's `write(op, fn, landed)`: run a write against the current writable
    /// mount, publish `wardrobe` when it landed, and log the location-write
    /// DEBUG on every write that returned (see the module doc).
    fn write<T>(
        &self,
        main: &Connection,
        op: &'static str,
        f: impl FnOnce(&WardrobeMount) -> Result<T, WardrobeLocationError>,
        landed: impl FnOnce(&T) -> bool,
    ) -> Result<T, WardrobeLocationError> {
        let result = f(&self.writable_mount(main)?)?;
        let published = landed(&result);
        if published {
            crate::realtime::bus::publish_realtime(
                crate::realtime::types::RealtimeTopic::Wardrobe,
                None,
            );
        }
        tracing::debug!(
            op,
            mountPointId = %self.mount.mount_point_id,
            scope = self.mount.scope.as_str(),
            published,
            context = "wardrobe",
            "[WardrobeLocation] Location write"
        );
        Ok(result)
    }

    pub fn create(
        &self,
        main: &Connection,
        mount: &Connection,
        item: &WardrobeItem,
    ) -> Result<WardrobeItem, WardrobeLocationError> {
        self.write(
            main,
            "create",
            |target| create_in_mount(main, mount, target, item),
            |_| true,
        )
    }

    /// `update_many` with one patch.
    pub fn update(
        &self,
        main: &Connection,
        mount: &Connection,
        id: &str,
        patch: &WardrobePatch,
    ) -> Result<Option<WardrobeItem>, WardrobeLocationError> {
        let mut updated = self.update_many(
            main,
            mount,
            &[WardrobeItemPatch {
                id: id.to_string(),
                patch: patch.clone(),
            }],
            UpdateManyOptions::default(),
        )?;
        Ok(if updated.is_empty() {
            None
        } else {
            Some(updated.remove(0))
        })
    }

    /// Patch several items with one read and one projection; the updated items
    /// in patch order (an id not in the folder is skipped, or refuses the batch
    /// with `require_all`). Identity and timestamps are the writer's, never a
    /// patch's — [`WardrobePatch`] cannot carry them by construction.
    pub fn update_many(
        &self,
        main: &Connection,
        mount: &Connection,
        patches: &[WardrobeItemPatch],
        opts: UpdateManyOptions,
    ) -> Result<Vec<WardrobeItem>, WardrobeLocationError> {
        self.write(
            main,
            "updateMany",
            |target| update_many_in_mount(main, mount, target, patches, opts),
            |updated| !updated.is_empty(),
        )
    }

    pub fn delete(
        &self,
        main: &Connection,
        mount: &Connection,
        id: &str,
    ) -> Result<bool, WardrobeLocationError> {
        self.write(
            main,
            "delete",
            |target| delete_in_mount(main, mount, target, id),
            |deleted| *deleted,
        )
    }

    /// Every folder under `Wardrobe/` (empty ones included), with direct item
    /// counts. A READ — no tombstone.
    pub fn list_folders(&self, mount: &Connection) -> Result<Vec<WardrobeFolder>, DbError> {
        list_folders_in_mount(mount, &self.mount)
    }

    /// Resolve an operator's requested folder against this wardrobe's folders
    /// (`resolve_wardrobe_folder`): an existing folder as stored, else the new
    /// part normalised. Empty / whitespace → `''`.
    pub fn resolve_folder(
        &self,
        mount: &Connection,
        requested: &str,
    ) -> Result<String, WardrobeFolderPathError> {
        if crate::jsstr::js_trim(requested).is_empty() {
            return Ok(String::new());
        }
        resolve_wardrobe_folder(
            &list_folder_paths_in_mount(mount, &self.mount),
            Some(requested),
        )
    }

    /// Create a folder (normalised path); an existing one in any casing is
    /// answered.
    pub fn create_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<WardrobeFolder, WardrobeLocationError> {
        self.write(
            main,
            "createFolder",
            |target| create_folder_in_mount(mount, target, path),
            |_| true,
        )
    }

    /// Rename a folder in place to `new_name` (one normalised segment).
    pub fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<WardrobeFolder, WardrobeLocationError> {
        self.write(
            main,
            "renameFolder",
            |target| rename_folder_in_mount(mount, target, path, new_name),
            |_| true,
        )
    }

    /// Delete a folder, sending its items and subfolders up to its parent.
    pub fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, WardrobeLocationError> {
        self.write(
            main,
            "deleteFolder",
            |target| delete_folder_in_mount(main, mount, target, path),
            |_| true,
        )
    }

    /// The mount, checked for writing: `CharacterArchived` for an archived
    /// character. For writers that reach the folder without going through
    /// create/update/delete (dressing instructions, item pictures).
    pub fn writable_mount_point_id(
        &self,
        main: &Connection,
    ) -> Result<String, WardrobeLocationError> {
        Ok(self.writable_mount(main)?.mount_point_id)
    }
}

// === P4.D377 === the shared folder layer (`inventory::item_folder_location`).
impl ItemFolderLocation for WardrobeLocation {
    type Error = WardrobeLocationError;

    fn scope(&self) -> &'static str {
        self.scope.as_str()
    }
    fn mount_point_id(&self) -> &str {
        &self.mount_point_id
    }
    fn list_folders(
        &self,
        mount: &Connection,
    ) -> Result<Vec<crate::inventory::item_folders::ItemFolder>, DbError> {
        WardrobeLocation::list_folders(self, mount)
    }
    fn resolve_folder(&self, mount: &Connection, requested: &str) -> Result<String, Self::Error> {
        Ok(WardrobeLocation::resolve_folder(self, mount, requested)?)
    }
    fn create_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<WardrobeFolder, Self::Error> {
        WardrobeLocation::create_folder(self, main, mount, path)
    }
    fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<WardrobeFolder, Self::Error> {
        WardrobeLocation::rename_folder(self, main, mount, path, new_name)
    }
    fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, Self::Error> {
        WardrobeLocation::delete_folder(self, main, mount, path)
    }
}

impl ItemFilingLocation for WardrobeLocation {
    type Item = WardrobeItem;

    fn refile_items(
        &self,
        main: &Connection,
        mount: &Connection,
        item_ids: &[String],
        folder: &str,
    ) -> Result<Vec<WardrobeItem>, WardrobeLocationError> {
        let patches: Vec<WardrobeItemPatch> = item_ids
            .iter()
            .map(|id| WardrobeItemPatch {
                id: id.clone(),
                patch: WardrobePatch {
                    folder: Some(folder.to_string()),
                    ..WardrobePatch::default()
                },
            })
            .collect();
        self.update_many(
            main,
            mount,
            &patches,
            UpdateManyOptions { require_all: true },
        )
    }
}
// === P4.D377 ===

/// v4 `resolveWardrobeLocation(scope, id, repos, userId, { ensure })` —
/// resolve `{ scope, id }` to a location, or `None` when the owner is missing
/// (or, for a character, not `user_id`'s — R-E), when a non-General scope
/// arrives without an id, or when the backing store doesn't exist and `ensure`
/// is off (or can't be provisioned). ALWAYS logs the DEBUG `[WardrobeLocation]
/// Resolved wardrobe location`.
pub fn resolve_wardrobe_location(
    main: &Connection,
    mount: &Connection,
    scope: WardrobeScope,
    id: Option<&str>,
    user_id: &str,
    opts: ResolveWardrobeLocationOptions,
) -> Result<Option<WardrobeLocation>, DbError> {
    // v4 `id ?? null` — an empty string is falsy at the `!id` gate below AND
    // renders as `""` in the line (v4 logs the string it was given).
    let location = resolve(main, mount, scope, id, user_id, opts.ensure)?;
    tracing::debug!(
        scope = scope.as_str(),
        id = id.unwrap_or("null"),
        ensure = opts.ensure,
        found = location.is_some(),
        mountPointId = location
            .as_ref()
            .map(|l| l.mount_point_id.as_str())
            .unwrap_or("null"),
        context = "wardrobe",
        "[WardrobeLocation] Resolved wardrobe location"
    );
    Ok(location)
}

/// v4 `SHARED_WARDROBE_FOLDER` — the folder an ensuring resolve heals.
const SHARED_WARDROBE_FOLDER: &str = "Wardrobe";

fn owner_scope(scope: WardrobeScope) -> OwnerScope {
    match scope {
        WardrobeScope::Character => OwnerScope::Character,
        WardrobeScope::Group => OwnerScope::Group,
        WardrobeScope::Project => OwnerScope::Project,
        WardrobeScope::General => OwnerScope::General,
    }
}

/// v4 `resolve` over `resolveOwnerStore(scope, id, repos, userId, { ensure:
/// ensure ? { folder: 'Wardrobe', onFolderFailure: 'swallow' } : undefined,
/// archived: 'allow', context: 'wardrobe' })` — raw owner reads (R-F); General
/// → `GENERAL_WARDROBE_ORIGIN`; else the origin `{scope, id: ownerId, name:
/// store.owner.name}` (the RAW name).
fn resolve(
    main: &Connection,
    mount: &Connection,
    scope: WardrobeScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<WardrobeLocation>, DbError> {
    let Some(store) = resolve_owner_store(
        main,
        mount,
        owner_scope(scope),
        id,
        user_id,
        &ResolveOwnerStoreOptions {
            ensure: ensure.then_some(EnsureFolder {
                folder: SHARED_WARDROBE_FOLDER,
                on_folder_failure: OwnerFolderFailure::Swallow,
            }),
            archived: Archived::Allow,
            context: Some("wardrobe"),
        },
    )?
    else {
        return Ok(None);
    };
    if store.scope == OwnerScope::General {
        return Ok(Some(location_for_mount(
            WardrobeMount {
                mount_point_id: store.mount_point_id,
                scope: WardrobeScope::General,
                character_id: None,
            },
            None,
            general_wardrobe_origin(),
        )));
    }
    let owner_id = store.owner.id.clone().unwrap_or_default();
    let origin_scope = match scope {
        WardrobeScope::Character => WardrobeOriginScope::Character,
        WardrobeScope::Project => WardrobeOriginScope::Project,
        _ => WardrobeOriginScope::Group,
    };
    Ok(Some(location_for_mount(
        WardrobeMount {
            mount_point_id: store.mount_point_id,
            scope,
            character_id: (scope == WardrobeScope::Character).then(|| owner_id.clone()),
        },
        Some(&owner_id),
        WardrobeOrigin::new(origin_scope, &owner_id, &store.owner.name),
    )))
}

/// v4 `groupLocationsForCharacter(characterId)` — every group store a
/// character can wear from, as locations: one per mount, each tagged with its
/// group's origin, STRONGEST FIRST (membership order; the first group to claim
/// a shared store keeps it — `groupOriginMap(await groupMountsFor(id))`,
/// P4.D377 item 9). The read probe the transfer route uses.
pub fn group_locations_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Vec<WardrobeLocation> {
    let groups = group_mounts_for(main, mount, character_id, GroupMountsOptions::default());
    group_origin_map(&groups)
        .into_iter()
        .map(|(mount_point_id, origin)| {
            location_for_mount(
                WardrobeMount {
                    mount_point_id,
                    scope: WardrobeScope::Group,
                    character_id: None,
                },
                Some(&origin.id),
                WardrobeOrigin::new(WardrobeOriginScope::Group, &origin.id, &origin.name),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    /// A failed `Wardrobe/` heal on an ensuring resolve is reach's WARN
    /// (`[Reach] Failed to ensure Wardrobe folder {mountPointId, folder,
    /// context, error}` — v4 `resolveOwnerStore`'s `onFolderFailure:
    /// 'swallow'`, P4.D377 item 9) and the location is still answered. Posed
    /// with a mount index that has no folder table at all (the ensure's INSERT
    /// fails). The old `[SharedWardrobe]` line is gone.
    #[test]
    fn a_failed_folder_ensure_warns_through_reach_and_still_resolves_general() {
        let main = Connection::open_in_memory().unwrap();
        main.execute_batch(
            "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO instance_settings VALUES ('generalMountPointId', 'general-mp');",
        )
        .unwrap();
        let mount = Connection::open_in_memory().unwrap();

        let (resolved, lines) = captured_with(|| {
            resolve_wardrobe_location(
                &main,
                &mount,
                WardrobeScope::General,
                None,
                "user-1",
                ResolveWardrobeLocationOptions { ensure: true },
            )
        });
        let loc = resolved
            .expect("resolve")
            .expect("General resolves despite the failed ensure");
        assert_eq!(loc.mount_point_id, "general-mp");
        let warn = lines
            .iter()
            .find(|l| l.contains("[Reach] Failed to ensure Wardrobe folder"))
            .expect("the WARN fires");
        assert!(warn.starts_with("WARN "), "{warn}");
        assert!(
            warn.contains(" mountPointId=general-mp folder=Wardrobe context=wardrobe error="),
            "{warn}"
        );
        assert!(!lines.iter().any(|l| l.contains("[SharedWardrobe]")));
        assert!(
            lines.iter().any(
                |l| l.contains("[WardrobeLocation] Resolved wardrobe location")
                    && l.contains("found=true")
            ),
            "the resolve line still fires"
        );

        // The silence leg: a read probe never ensures, so nothing is WARNed.
        let (_, quiet) = captured_with(|| {
            resolve_wardrobe_location(
                &main,
                &mount,
                WardrobeScope::General,
                None,
                "user-1",
                ResolveWardrobeLocationOptions { ensure: false },
            )
        });
        assert!(!quiet.iter().any(|l| l.contains("[Reach]")));
    }
}

#[cfg(test)]
mod publish_tests {
    //! P4.D377 item 8 (v4 `600c2906f`, `lib/wardrobe/location.ts`
    //! `write(op, fn, landed)`): every write through a location logs the DEBUG
    //! `[WardrobeLocation] Location write {op, mountPointId, scope, published,
    //! context}`, and announces the collection-wide `wardrobe` topic ONLY when
    //! something landed (`published: true` ⇔ one hint); a batch or delete that
    //! matched nothing logs `published=false` and announces nothing; a write
    //! that THROWS (a vanished vault, the tombstone) logs nothing and announces
    //! nothing. The DEBUG runs on the writer thread, so the process-global
    //! capture reads it (memory note `capture-rig-writer-thread`).
    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::realtime::publish_sites::HintCapture;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const ITEM: &str = "a0000001-0000-4000-8000-0000000000bb";

    fn db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let db = Db::open(
            DbPaths {
                main: data.join("quilltap.db"),
                mount_index: Some(data.join("quilltap-mount-index.db")),
                llm_logs: None,
            },
            PEPPER,
        )
        .unwrap();
        (dir, db)
    }

    /// Run `op` over General's location inside one write.
    async fn in_general<F>(db: &Db, op: F) -> (String, String)
    where
        F: FnOnce(&Connection, &Connection, &WardrobeLocation) -> String + Send + 'static,
    {
        db.write(move |ws| {
            let main = ws.main().connection();
            let mount = ws.mount_index().expect("mount index").connection();
            let loc = resolve_wardrobe_location(
                main,
                mount,
                WardrobeScope::General,
                None,
                "u-1",
                ResolveWardrobeLocationOptions::default(),
            )?
            .expect("a provisioned instance has Quilltap General");
            Ok((op(main, mount, &loc), loc.mount_point_id.clone()))
        })
        .await
        .unwrap()
    }

    fn writes(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| l.contains("[WardrobeLocation] Location write"))
            .map(|l| l[l.find("[WardrobeLocation]").unwrap()..].to_string())
            .collect()
    }

    fn wardrobe() -> (String, Option<String>) {
        ("wardrobe".to_string(), None)
    }

    /// The location ops on DIRECT connections on the test thread — the DEBUG
    /// runs where the write runs, and the thread-local capture only sees this
    /// thread (memory note `capture-rig-writer-thread`, third trap).
    fn direct(dir: &tempfile::TempDir) -> (crate::db::Writer, crate::db::Writer) {
        let data = dir.path().join("data");
        (
            crate::db::Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap(),
            crate::db::Writer::open_writable_in(
                crate::db::table_shape::Partition::MountIndex,
                &data.join("quilltap-mount-index.db"),
                PEPPER,
            )
            .unwrap(),
        )
    }

    fn general(main: &Connection, mount: &Connection) -> WardrobeLocation {
        resolve_wardrobe_location(
            main,
            mount,
            WardrobeScope::General,
            None,
            "u-1",
            ResolveWardrobeLocationOptions::default(),
        )
        .unwrap()
        .expect("a provisioned instance has Quilltap General")
    }

    fn item() -> WardrobeItem {
        WardrobeItem::from_read_value(&json!({
            "id": ITEM, "title": "Bowler", "types": ["accessories"], "folder": "",
            "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-01T00:00:00.000Z",
        }))
    }

    #[test]
    fn every_write_logs_its_op_and_whether_it_published() {
        crate::test_support::global_capture::install();
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let (m, x) = direct(&dir);
        let (main, mount) = (m.connection(), x.connection());
        let loc = general(main, mount);
        let mp = loc.mount_point_id.clone();
        let line = |op: &str, published: bool| {
            format!(
                "[WardrobeLocation] Location write op={op} mountPointId={mp} scope=general \
                 published={published} context=wardrobe"
            )
        };
        let (out, lines) = crate::test_support::global_capture::capture(|| {
            let created = loc.create(main, mount, &item()).map(|i| i.id);
            let updated = loc
                .update_many(
                    main,
                    mount,
                    &[WardrobeItemPatch {
                        id: "nope".into(),
                        patch: WardrobePatch::default(),
                    }],
                    UpdateManyOptions::default(),
                )
                .map(|u| u.len());
            let missed = loc.delete(main, mount, "nope");
            let moved = loc
                .update(
                    main,
                    mount,
                    ITEM,
                    &WardrobePatch {
                        folder: Some("Hats".to_string()),
                        ..WardrobePatch::default()
                    },
                )
                .map(|u| u.map(|i| i.folder));
            let made = loc.create_folder(main, mount, "Caps").map(|f| f.path);
            let renamed = loc
                .rename_folder(main, mount, "Caps", "Bonnets")
                .map(|f| f.path);
            let dropped = loc
                .delete_folder(main, mount, "Bonnets")
                .map(|d| d.moved_items);
            let deleted = loc.delete(main, mount, ITEM);
            format!(
                "{created:?} {updated:?} {missed:?} {moved:?} {made:?} {renamed:?} {dropped:?} {deleted:?}"
            )
        });
        assert_eq!(
            out,
            format!(
                "Ok({ITEM:?}) Ok(0) Ok(false) Ok(Some(\"Hats\")) Ok(\"Caps\") Ok(\"Bonnets\") Ok(0) Ok(true)"
            )
        );
        assert_eq!(
            writes(&lines),
            vec![
                line("create", true),
                line("updateMany", false),
                line("delete", false),
                line("updateMany", true),
                line("createFolder", true),
                line("renameFolder", true),
                line("deleteFolder", true),
                line("delete", true),
            ]
        );
    }

    #[tokio::test]
    async fn a_landed_write_announces_wardrobe_and_a_miss_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, db) = db();
        cap.arm_writer_thread(&db).await;
        let (created, _) = in_general(&db, |main, mount, loc| {
            format!("{:?}", loc.create(main, mount, &item()).map(|i| i.id))
        })
        .await;
        assert_eq!(created, format!("Ok({ITEM:?})"));
        assert_eq!(cap.drain_expecting(1).await, vec![wardrobe()]);
        let (missed, _) = in_general(&db, |main, mount, loc| {
            let updated = loc
                .update_many(
                    main,
                    mount,
                    &[WardrobeItemPatch {
                        id: "nope".into(),
                        patch: WardrobePatch::default(),
                    }],
                    UpdateManyOptions::default(),
                )
                .map(|u| u.len());
            let deleted = loc.delete(main, mount, "nope");
            format!("{updated:?} {deleted:?}")
        })
        .await;
        assert_eq!(missed, "Ok(0) Ok(false)");
        assert_eq!(cap.drain().await, Vec::<(String, Option<String>)>::new());
        let (made, _) = in_general(&db, |main, mount, loc| {
            format!(
                "{:?}",
                loc.create_folder(main, mount, "Caps").map(|f| f.path)
            )
        })
        .await;
        assert_eq!(made, "Ok(\"Caps\")");
        assert_eq!(cap.drain_expecting(1).await, vec![wardrobe()]);
    }

    #[test]
    fn a_write_that_throws_logs_nothing() {
        crate::test_support::global_capture::install();
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let (m, x) = direct(&dir);
        // A character location whose vault is gone: the write-time re-resolve
        // throws `Character … has no linked vault` before anything is written.
        let loc = location_for_mount(
            WardrobeMount {
                mount_point_id: "mp-gone".into(),
                scope: WardrobeScope::Character,
                character_id: Some("c-gone".into()),
            },
            Some("c-gone"),
            WardrobeOrigin::new(WardrobeOriginScope::Character, "c-gone", "Gone"),
        );
        let (out, lines) = crate::test_support::global_capture::capture(|| {
            loc.create_folder(m.connection(), x.connection(), "Hats")
                .map(|f| f.path)
                .map_err(|e| e.message())
        });
        assert_eq!(out, Err("Character c-gone has no linked vault".to_string()));
        assert!(writes(&lines).is_empty(), "{lines:?}");
    }
}
