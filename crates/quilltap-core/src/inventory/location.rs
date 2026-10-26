//! Inventory Location — the one answer to "where does this object live, and
//! how do I read and write there" (v4 `lib/inventory/location.ts`,
//! `4fbdad186`; P4.D324; the `db::wardrobe_location` twin).
//!
//! A location is one tier's `Inventory/` folder: a character's vault,
//! Quilltap General, a project's official store or a group's official store.
//! Routes, transfers, pictures and equip address a tier through this; nobody
//! switches on scope to pick a writer.
//!
//! `ensure: true` provisions a missing project/group store and its
//! `Inventory/` folder, for callers about to write; a read probe passes
//! `false` and gets `None` for a store that does not exist yet. An archived
//! character's location still reads, but every write through it refuses with
//! [`InventoryError::CharacterArchived`] (the write-time re-resolve).
//!
//! Every op takes the connections it touches (`main` for the tombstone
//! re-resolve, `mount` for the document store) — v4's closures over the
//! repository container become explicit arguments.

use rusqlite::Connection;
use serde_json::Value;

use super::errors::InventoryError;
use super::folders::{resolve_inventory_folder, InventoryFolder, INVENTORY_FOLDER};
use super::types::{InventoryFields, InventoryOrigin, InventoryRecord, InventoryScope};
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::ensure_official_store::{ensure_owner_official_store, StoreOwnerKind};
use crate::db::groups::GroupsRepository;
use crate::db::inventory_writes::{
    create_inventory_folder_in_mount, create_inventory_in_mount, delete_inventory_folder_in_mount,
    delete_inventory_in_mount, list_inventory_folder_paths_in_mount,
    list_inventory_folders_in_mount, read_mount_inventory, rename_inventory_folder_in_mount,
    resolve_inventory_mount, update_inventory_many_in_mount, InventoryMount, InventoryRecordPatch,
};
use crate::db::item_folder_ops::{DeletedFolder, VaultFolderMountScope};
use crate::db::projects::ProjectsRepository;
use crate::db::tiered_mount_pool::resolve_group_mounts_for_character;
use crate::db::{characters_read, instance_settings, DbError};

impl InventoryScope {
    /// The folder mount's scope for this tier.
    pub fn mount_scope(self) -> VaultFolderMountScope {
        match self {
            InventoryScope::Character => VaultFolderMountScope::Character,
            InventoryScope::Group => VaultFolderMountScope::Group,
            InventoryScope::Project => VaultFolderMountScope::Project,
            InventoryScope::General => VaultFolderMountScope::General,
        }
    }

    /// The tier a folder mount belongs to.
    pub fn of_mount(scope: VaultFolderMountScope) -> Self {
        match scope {
            VaultFolderMountScope::Character => InventoryScope::Character,
            VaultFolderMountScope::Group => InventoryScope::Group,
            VaultFolderMountScope::Project => InventoryScope::Project,
            VaultFolderMountScope::General => InventoryScope::General,
        }
    }
}

/// What Quilltap General's objects are tagged with (v4
/// `GENERAL_INVENTORY_ORIGIN`).
pub fn general_inventory_origin() -> InventoryOrigin {
    InventoryOrigin {
        scope: InventoryScope::General,
        id: None,
        name: "Quilltap General".to_string(),
    }
}

/// One tier's `Inventory/` folder with its origin (v4 `InventoryLocation`).
#[derive(Debug, Clone)]
pub struct InventoryLocation {
    pub scope: InventoryScope,
    /// Owner id; `None` for General.
    pub id: Option<String>,
    /// The mount holding the `Inventory/` folder.
    pub mount_point_id: String,
    /// The owning character — `character` scope only.
    pub character_id: Option<String>,
    /// What a collection read tags this tier's objects with.
    pub origin: InventoryOrigin,
    /// The backing mount as resolved (a READ mount — no tombstone check).
    pub mount: InventoryMount,
}

/// A key naming one folder, for "same source and destination?" checks (v4
/// `locationKey` — `${scope}:${mountPointId}`).
pub fn location_key(loc: &InventoryLocation) -> String {
    format!("{}:{}", loc.scope.as_str(), loc.mount_point_id)
}

/// Idempotent: ensure a mount's `Inventory/` folder exists; a failure is
/// logged, not thrown (v4 `ensureInventoryFolder` — WARN `[InventoryLocation]
/// Failed to ensure Inventory folder {mountPointId, context, error}`).
pub fn ensure_inventory_folder(mount: &Connection, mount_point_id: &str) {
    if let Err(error) =
        DocMountFileLinksRepository::new(mount).ensure_folder_path(mount_point_id, INVENTORY_FOLDER)
    {
        tracing::warn!(
            mountPointId = %mount_point_id,
            context = "inventory",
            error = %crate::db::fallback::error_text(&error),
            "[InventoryLocation] Failed to ensure Inventory folder"
        );
    }
}

/// Identity and timestamps are the writer's, never a patch's (v4
/// `cleanPatch`).
fn clean_patch(p: &InventoryRecordPatch) -> InventoryRecordPatch {
    let mut patch = p.patch.clone();
    for key in ["id", "createdAt", "updatedAt"] {
        patch.shift_remove(key);
    }
    InventoryRecordPatch {
        id: p.id.clone(),
        patch,
        remove: p.remove.clone(),
    }
}

/// Build a location over a folder (v4 `inventoryLocationForMount`). Writes
/// re-resolve a character's mount at write time so the tombstone check sees
/// the character as it is now.
pub fn inventory_location_for_mount(
    mount: InventoryMount,
    id: Option<&str>,
    origin: InventoryOrigin,
) -> InventoryLocation {
    InventoryLocation {
        scope: InventoryScope::of_mount(mount.scope),
        id: id.map(str::to_string),
        mount_point_id: mount.mount_point_id.clone(),
        character_id: mount.character_id.clone(),
        origin,
        mount,
    }
}

impl InventoryLocation {
    /// v4's `writableMount()`: a shared tier's mount as given; a character's
    /// RE-RESOLVED (`CharacterArchived` for the tombstone; `Character ${id}
    /// has no linked vault` for a vanished one).
    fn writable_mount(&self, main: &Connection) -> Result<InventoryMount, InventoryError> {
        if self.mount.scope != VaultFolderMountScope::Character {
            return Ok(self.mount.clone());
        }
        let character_id = self.mount.character_id.clone().unwrap_or_default();
        match resolve_inventory_mount(main, Some(&character_id))? {
            Some(current) => Ok(current),
            None => Err(InventoryError::Other(format!(
                "Character {character_id} has no linked vault"
            ))),
        }
    }

    /// Every record in the folder; archived ones only when asked. Unresolved.
    pub fn read_items(
        &self,
        mount: &Connection,
        include_archived: bool,
    ) -> Result<Vec<InventoryRecord>, DbError> {
        let records = read_mount_inventory(mount, &self.mount)?;
        Ok(if include_archived {
            records
        } else {
            records.into_iter().filter(|r| !r.is_archived()).collect()
        })
    }

    /// One record from the folder (archived included), or `None`.
    pub fn find_item(
        &self,
        mount: &Connection,
        id: &str,
    ) -> Result<Option<InventoryRecord>, DbError> {
        Ok(read_mount_inventory(mount, &self.mount)?
            .into_iter()
            .find(|r| r.id() == id))
    }

    pub fn create(
        &self,
        main: &Connection,
        mount: &Connection,
        record: &InventoryRecord,
    ) -> Result<InventoryRecord, InventoryError> {
        Ok(create_inventory_in_mount(
            mount,
            &self.writable_mount(main)?,
            record,
        )?)
    }

    /// `update_many` with one patch; `None` when the id isn't there.
    pub fn update(
        &self,
        main: &Connection,
        mount: &Connection,
        id: &str,
        patch: serde_json::Map<String, Value>,
        remove: &[String],
    ) -> Result<Option<InventoryRecord>, InventoryError> {
        let mut updated = self.update_many(
            main,
            mount,
            &[InventoryRecordPatch {
                id: id.to_string(),
                patch,
                remove: remove.to_vec(),
            }],
            false,
        )?;
        Ok(if updated.is_empty() {
            None
        } else {
            Some(updated.remove(0))
        })
    }

    /// Patch several records with one read and one projection.
    pub fn update_many(
        &self,
        main: &Connection,
        mount: &Connection,
        patches: &[InventoryRecordPatch],
        require_all: bool,
    ) -> Result<Vec<InventoryRecord>, InventoryError> {
        let target = self.writable_mount(main)?;
        let cleaned: Vec<InventoryRecordPatch> = patches.iter().map(clean_patch).collect();
        Ok(update_inventory_many_in_mount(
            mount,
            &target,
            &cleaned,
            require_all,
        )?)
    }

    /// Delete records; `follow_on` patches the survivors in the same
    /// projection (a deleted kit member dropped from the kits that named it).
    pub fn delete(
        &self,
        main: &Connection,
        mount: &Connection,
        ids: &[String],
        follow_on: impl FnOnce(&[InventoryRecord]) -> Vec<InventoryRecordPatch>,
    ) -> Result<Vec<String>, InventoryError> {
        let target = self.writable_mount(main)?;
        Ok(delete_inventory_in_mount(mount, &target, ids, follow_on)?)
    }

    /// Every folder under `Inventory/` (empty ones included), with direct
    /// object counts. A READ — no tombstone.
    pub fn list_folders(&self, mount: &Connection) -> Result<Vec<InventoryFolder>, DbError> {
        list_inventory_folders_in_mount(mount, &self.mount)
    }

    /// Resolve an operator's requested folder against this tier's folders.
    /// Empty / whitespace → `''` with no read.
    pub fn resolve_folder(
        &self,
        mount: &Connection,
        requested: &str,
    ) -> Result<String, InventoryError> {
        if crate::jsstr::js_trim(requested).is_empty() {
            return Ok(String::new());
        }
        Ok(resolve_inventory_folder(
            &list_inventory_folder_paths_in_mount(mount, &self.mount),
            Some(requested),
        )?)
    }

    pub fn create_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<InventoryFolder, InventoryError> {
        let target = self.writable_mount(main)?;
        Ok(create_inventory_folder_in_mount(mount, &target, path)?)
    }

    pub fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<InventoryFolder, InventoryError> {
        let target = self.writable_mount(main)?;
        Ok(rename_inventory_folder_in_mount(
            mount, &target, path, new_name,
        )?)
    }

    pub fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, InventoryError> {
        let target = self.writable_mount(main)?;
        Ok(delete_inventory_folder_in_mount(
            main, mount, &target, path,
        )?)
    }

    /// The mount, checked for writing (`CharacterArchived` for an archived
    /// character) — for writers that reach the folder without going through
    /// create / update / delete (pictures).
    pub fn writable_mount_point_id(&self, main: &Connection) -> Result<String, InventoryError> {
        Ok(self.writable_mount(main)?.mount_point_id)
    }
}

/// v4 `ResolveInventoryLocationOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolveInventoryLocationOptions {
    /// Provision a missing project/group store and its folder. For writers.
    pub ensure: bool,
}

/// Resolve `{ scope, id }` to a location, or `None` when the owner is missing
/// (or, for a character, not `user_id`'s), when a non-General scope arrives
/// without an id, or when the store does not exist and `ensure` is off (v4
/// `resolveInventoryLocation`). ALWAYS logs the DEBUG `[InventoryLocation]
/// Resolved inventory location` (one line per call that returns).
pub fn resolve_inventory_location(
    main: &Connection,
    mount: &Connection,
    scope: InventoryScope,
    id: Option<&str>,
    user_id: &str,
    opts: ResolveInventoryLocationOptions,
) -> Result<Option<InventoryLocation>, DbError> {
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
        op = "resolve",
        context = "inventory",
        "[InventoryLocation] Resolved inventory location"
    );
    Ok(location)
}

fn resolve(
    main: &Connection,
    mount: &Connection,
    scope: InventoryScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<InventoryLocation>, DbError> {
    if scope == InventoryScope::General {
        let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
            return Ok(None);
        };
        if ensure {
            ensure_inventory_folder(mount, &mount_point_id);
        }
        return Ok(Some(inventory_location_for_mount(
            InventoryMount {
                mount_point_id,
                scope: VaultFolderMountScope::General,
                character_id: None,
            },
            None,
            general_inventory_origin(),
        )));
    }

    // v4 `if (!id) return null` — an empty string is falsy too.
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    if scope == InventoryScope::Character {
        // `repos.characters.findByIdRaw` — the fallback `_findById`.
        let Some(character) = characters_read::find_by_id_raw_or_none(main, id) else {
            return Ok(None);
        };
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
            .unwrap_or_default()
            .to_string();
        return Ok(Some(inventory_location_for_mount(
            InventoryMount {
                mount_point_id: mount_point_id.to_string(),
                scope: VaultFolderMountScope::Character,
                character_id: Some(character_id.clone()),
            },
            Some(&character_id),
            InventoryOrigin {
                scope: InventoryScope::Character,
                id: Some(character_id.clone()),
                name,
            },
        )));
    }

    let (owner, kind, fallback_name) = if scope == InventoryScope::Project {
        (
            ProjectsRepository::new(main, mount)
                .find_by_id(id)
                .map_err(|e| e.into_db())?,
            StoreOwnerKind::Project,
            "Project",
        )
    } else {
        (
            GroupsRepository::new(main, mount)
                .find_by_id(id)
                .map_err(|e| e.into_db())?,
            StoreOwnerKind::Group,
            "Group",
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
        .unwrap_or_default()
        .to_string();
    let name = if raw_name.is_empty() {
        fallback_name.to_string()
    } else {
        raw_name.clone()
    };
    let mount_point_id = if ensure {
        let Some(ensured) = ensure_owner_official_store(main, mount, kind, &owner_id, &name)?
        else {
            return Ok(None);
        };
        ensure_inventory_folder(mount, &ensured.mount_point_id);
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
    Ok(Some(inventory_location_for_mount(
        InventoryMount {
            mount_point_id,
            scope: scope.mount_scope(),
            character_id: None,
        },
        Some(&owner_id),
        InventoryOrigin {
            scope,
            id: Some(owner_id.clone()),
            name: raw_name,
        },
    )))
}

/// Every group store a character can reach, as locations — one per mount,
/// tagged with its group, in the order the group tier shadows (later wins)
/// (v4 `groupLocationsForCharacter`).
pub fn group_locations_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Vec<InventoryLocation> {
    resolve_group_mounts_for_character(main, mount, character_id)
        .into_iter()
        .flat_map(|g| {
            g.mount_point_ids
                .into_iter()
                .map(|mount_point_id| {
                    inventory_location_for_mount(
                        InventoryMount {
                            mount_point_id,
                            scope: VaultFolderMountScope::Group,
                            character_id: None,
                        },
                        Some(&g.group.id),
                        InventoryOrigin {
                            scope: InventoryScope::Group,
                            id: Some(g.group.id.clone()),
                            name: g.group.name.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}
