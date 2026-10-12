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

use rusqlite::Connection;

use super::archetype_wardrobe::{
    ensure_shared_wardrobe_folder, general_wardrobe_origin, WardrobeOrigin, WardrobeOriginScope,
};
use super::characters_read;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::ensure_official_store::{ensure_owner_official_store, StoreOwnerKind};
use super::groups::GroupsRepository;
use super::instance_settings;
use super::projects::ProjectsRepository;
use super::tiered_mount_pool::resolve_group_mounts_for_character;
use super::vault_wardrobe_public::{
    create_folder_in_mount, create_in_mount, delete_folder_in_mount, delete_in_mount,
    list_folder_paths_in_mount, list_folders_in_mount, read_mount_items, rename_folder_in_mount,
    resolve_wardrobe_mount, update_many_in_mount, DeletedFolder, UpdateManyOptions,
    WardrobeItemPatch, WardrobePatch, WardrobePublicError,
};
pub use super::vault_wardrobe_public::{WardrobeMount, WardrobeScope};
use super::DbError;
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe_folders::{
    conflict_default_message, not_found_message, not_in_location_message, resolve_wardrobe_folder,
    WardrobeFolder, WardrobeFolderPathError,
};
use serde_json::Value;

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

    pub fn create(
        &self,
        main: &Connection,
        mount: &Connection,
        item: &WardrobeItem,
    ) -> Result<WardrobeItem, WardrobeLocationError> {
        create_in_mount(main, mount, &self.writable_mount(main)?, item)
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
        update_many_in_mount(main, mount, &self.writable_mount(main)?, patches, opts)
    }

    pub fn delete(
        &self,
        main: &Connection,
        mount: &Connection,
        id: &str,
    ) -> Result<bool, WardrobeLocationError> {
        delete_in_mount(main, mount, &self.writable_mount(main)?, id)
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
        create_folder_in_mount(mount, &self.writable_mount(main)?, path)
    }

    /// Rename a folder in place to `new_name` (one normalised segment).
    pub fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<WardrobeFolder, WardrobeLocationError> {
        rename_folder_in_mount(mount, &self.writable_mount(main)?, path, new_name)
    }

    /// Delete a folder, sending its items and subfolders up to its parent.
    pub fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, WardrobeLocationError> {
        delete_folder_in_mount(main, mount, &self.writable_mount(main)?, path)
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

fn resolve(
    main: &Connection,
    mount: &Connection,
    scope: WardrobeScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<WardrobeLocation>, DbError> {
    let links = DocMountFileLinksRepository::new(mount);
    if scope == WardrobeScope::General {
        let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
            return Ok(None);
        };
        if ensure {
            ensure_shared_folder_soft(&links, &mount_point_id);
        }
        return Ok(Some(location_for_mount(
            WardrobeMount {
                mount_point_id,
                scope: WardrobeScope::General,
                character_id: None,
            },
            None,
            general_wardrobe_origin(),
        )));
    }

    // v4 `if (!id) return null` — an empty string is falsy too.
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    if scope == WardrobeScope::Character {
        let Some(character) = characters_read::find_by_id_raw(main, id)? else {
            return Ok(None);
        };
        // The ownership gate (R-E).
        if character.get("userId").and_then(Value::as_str) != Some(user_id) {
            return Ok(None);
        }
        let Some(mount_point_id) = character
            .get("characterDocumentMountPointId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return Ok(None);
        };
        let character_id = character
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string();
        let name = character
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Ok(Some(location_for_mount(
            WardrobeMount {
                mount_point_id: mount_point_id.to_string(),
                scope: WardrobeScope::Character,
                character_id: Some(character_id.clone()),
            },
            Some(&character_id),
            WardrobeOrigin::new(WardrobeOriginScope::Character, &character_id, name),
        )));
    }

    let (owner, kind, fallback_name, origin_scope) = if scope == WardrobeScope::Project {
        (
            ProjectsRepository::new(main, mount)
                .find_by_id(id)
                .map_err(|e| e.into_db())?,
            StoreOwnerKind::Project,
            "Project",
            WardrobeOriginScope::Project,
        )
    } else {
        (
            GroupsRepository::new(main, mount)
                .find_by_id(id)
                .map_err(|e| e.into_db())?,
            StoreOwnerKind::Group,
            "Group",
            WardrobeOriginScope::Group,
        )
    };
    let Some(owner) = owner else {
        return Ok(None);
    };
    let owner_id = owner
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(id)
        .to_string();
    // The RAW name goes on the origin; the fallback only names the store.
    let raw_name = owner
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = if raw_name.is_empty() {
        fallback_name
    } else {
        raw_name
    };

    let mount_point_id = if ensure {
        let Some(ensured) = ensure_owner_official_store(main, mount, kind, &owner_id, name)? else {
            return Ok(None);
        };
        ensure_shared_folder_soft(&links, &ensured.mount_point_id);
        Some(ensured.mount_point_id)
    } else {
        owner
            .get("officialMountPointId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let Some(mount_point_id) = mount_point_id else {
        return Ok(None);
    };

    Ok(Some(location_for_mount(
        WardrobeMount {
            mount_point_id,
            scope,
            character_id: None,
        },
        Some(&owner_id),
        WardrobeOrigin::new(origin_scope, &owner_id, raw_name),
    )))
}

/// v4 `ensureSharedWardrobeFolder(mountPointId)` as the location calls it: the
/// folder ensure is best-effort — a failure is WARNed (`[SharedWardrobe] Failed
/// to ensure Wardrobe folder`, v4's keys and order) and the resolve goes on to
/// answer the location; the write that follows will surface the store's state
/// itself. (`archetype_wardrobe::ensure_shared_wardrobe_folder` is the bare
/// ensure; v4's catch lives in `shared-wardrobe.ts`, so it is carried here at
/// the one caller that must not fail on it.)
fn ensure_shared_folder_soft(links: &DocMountFileLinksRepository, mount_point_id: &str) {
    if let Err(error) = ensure_shared_wardrobe_folder(links, mount_point_id) {
        tracing::warn!(
            mountPointId = %mount_point_id,
            context = "wardrobe",
            error = %crate::db::fallback::error_text(&error),
            "[SharedWardrobe] Failed to ensure Wardrobe folder"
        );
    }
}

/// v4 `groupLocationsForCharacter(characterId)` — every group store a
/// character can wear from, as locations: one per mount, each tagged with its
/// group's origin, in the order the group tier shadows (later wins). The read
/// probe the transfer route and the pool both use.
pub fn group_locations_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Vec<WardrobeLocation> {
    resolve_group_mounts_for_character(main, mount, character_id)
        .into_iter()
        .flat_map(|g| {
            g.mount_point_ids
                .into_iter()
                .map(move |mount_point_id| {
                    location_for_mount(
                        WardrobeMount {
                            mount_point_id,
                            scope: WardrobeScope::Group,
                            character_id: None,
                        },
                        Some(&g.group.id),
                        WardrobeOrigin::new(WardrobeOriginScope::Group, &g.group.id, &g.group.name),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::captured_with;

    /// v4 `shared-wardrobe.ts`: a failed `Wardrobe/` ensure is WARNed and the
    /// location is still answered — the resolve never fails on it. Posed with a
    /// mount index that has no folder table at all (the ensure's INSERT fails).
    #[test]
    fn a_failed_folder_ensure_warns_and_still_resolves_general() {
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
            .find(|l| l.contains("[SharedWardrobe] Failed to ensure Wardrobe folder"))
            .expect("the WARN fires");
        assert!(warn.starts_with("WARN "), "{warn}");
        assert!(
            warn.contains(" mountPointId=general-mp context=wardrobe error="),
            "{warn}"
        );
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
        assert!(!quiet.iter().any(|l| l.contains("[SharedWardrobe]")));
    }
}
