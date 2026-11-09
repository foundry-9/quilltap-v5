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
//! Owners are read RAW through `db::reach::resolve_owner_store` (v4
//! `0c47f4539`; P4.D377 item 9, the survey's R-F); an ensuring resolve's
//! folder heal is reach's, and its failure is reach's `[Reach] Failed to
//! ensure Inventory folder` WARN (the old `[InventoryLocation]` WARN is gone).
//! The folder half implements the shared
//! [`ItemFolderLocation`](super::item_folder_location::ItemFolderLocation).
//!
//! Every op takes the connections it touches (`main` for the tombstone
//! re-resolve, `mount` for the document store) — v4's closures over the
//! repository container become explicit arguments.

use rusqlite::Connection;
use serde_json::Value;

use super::errors::InventoryError;
use super::folders::{resolve_inventory_folder, InventoryFolder, INVENTORY_FOLDER};
use super::item_folder_location::{ItemFilingLocation, ItemFolderLocation};
use super::types::{InventoryFields, InventoryOrigin, InventoryRecord, InventoryScope};
use crate::db::inventory_writes::{
    create_inventory_folder_in_mount, create_inventory_in_mount, delete_inventory_folder_in_mount,
    delete_inventory_in_mount, list_inventory_folder_paths_in_mount,
    list_inventory_folders_in_mount, read_mount_inventory, rename_inventory_folder_in_mount,
    resolve_inventory_mount, update_inventory_many_in_mount, InventoryMount, InventoryRecordPatch,
};
use crate::db::item_folder_ops::{DeletedFolder, VaultFolderMountScope};
use crate::db::reach::{
    group_mounts_for, group_origin_map, resolve_owner_store, Archived, EnsureFolder,
    OwnerFolderFailure, OwnerScope, ResolveOwnerStoreOptions,
};
use crate::db::tiered_mount_pool::GroupMountsOptions;
use crate::db::DbError;

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

/// Tell the open dialogs that something landed (v4's `write(fn, landed)`
/// wrapper, `0276d10d9`): every landed write through a location publishes
/// `inventory` — create, a non-empty `update_many` (`update` rides it), a
/// non-empty delete, and the three folder writes. The hint is a synchronous,
/// coalescing, fire-and-forget call, safe inside a write closure.
fn landed() {
    crate::realtime::bus::publish_realtime(crate::realtime::types::RealtimeTopic::Inventory, None);
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
        let created = create_inventory_in_mount(mount, &self.writable_mount(main)?, record)?;
        landed();
        Ok(created)
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
        let updated = update_inventory_many_in_mount(mount, &target, &cleaned, require_all)?;
        // A batch that matched nothing changed nothing.
        if !updated.is_empty() {
            landed();
        }
        Ok(updated)
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
        let deleted = delete_inventory_in_mount(mount, &target, ids, follow_on)?;
        if !deleted.is_empty() {
            landed();
        }
        Ok(deleted)
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
        let folder = create_inventory_folder_in_mount(mount, &target, path)?;
        landed();
        Ok(folder)
    }

    pub fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<InventoryFolder, InventoryError> {
        let target = self.writable_mount(main)?;
        let folder = rename_inventory_folder_in_mount(mount, &target, path, new_name)?;
        landed();
        Ok(folder)
    }

    pub fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, InventoryError> {
        let target = self.writable_mount(main)?;
        let deleted = delete_inventory_folder_in_mount(main, mount, &target, path)?;
        landed();
        Ok(deleted)
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

/// v4 `resolve` over `resolveOwnerStore(scope, id, repos, userId, { ensure:
/// ensure ? { folder: INVENTORY_FOLDER, onFolderFailure: 'swallow' } :
/// undefined, archived: 'allow', context: 'inventory' })` — raw owner reads
/// (R-F); the origin carries the RAW owner name.
fn resolve(
    main: &Connection,
    mount: &Connection,
    scope: InventoryScope,
    id: Option<&str>,
    user_id: &str,
    ensure: bool,
) -> Result<Option<InventoryLocation>, DbError> {
    let owner_scope = match scope {
        InventoryScope::Character => OwnerScope::Character,
        InventoryScope::Group => OwnerScope::Group,
        InventoryScope::Project => OwnerScope::Project,
        InventoryScope::General => OwnerScope::General,
    };
    let Some(store) = resolve_owner_store(
        main,
        mount,
        owner_scope,
        id,
        user_id,
        &ResolveOwnerStoreOptions {
            ensure: ensure.then_some(EnsureFolder {
                folder: INVENTORY_FOLDER,
                on_folder_failure: OwnerFolderFailure::Swallow,
            }),
            archived: Archived::Allow,
            context: Some("inventory"),
        },
    )?
    else {
        return Ok(None);
    };
    if store.scope == OwnerScope::General {
        return Ok(Some(inventory_location_for_mount(
            InventoryMount {
                mount_point_id: store.mount_point_id,
                scope: VaultFolderMountScope::General,
                character_id: None,
            },
            None,
            general_inventory_origin(),
        )));
    }
    let owner_id = store.owner.id.clone().unwrap_or_default();
    Ok(Some(inventory_location_for_mount(
        InventoryMount {
            mount_point_id: store.mount_point_id,
            scope: scope.mount_scope(),
            character_id: (scope == InventoryScope::Character).then(|| owner_id.clone()),
        },
        Some(&owner_id),
        InventoryOrigin {
            scope,
            id: Some(owner_id.clone()),
            name: store.owner.name,
        },
    )))
}

/// Every group store a character can reach, as locations — one per mount,
/// tagged with its group, STRONGEST FIRST (membership order; the first group
/// to claim a shared store keeps it) (v4 `groupLocationsForCharacter` =
/// `groupOriginMap(await groupMountsFor(id))`, P4.D377 item 9).
pub fn group_locations_for_character(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
) -> Vec<InventoryLocation> {
    let groups = group_mounts_for(main, mount, character_id, GroupMountsOptions::default());
    group_origin_map(&groups)
        .into_iter()
        .map(|(mount_point_id, origin)| {
            inventory_location_for_mount(
                InventoryMount {
                    mount_point_id,
                    scope: VaultFolderMountScope::Group,
                    character_id: None,
                },
                Some(&origin.id),
                InventoryOrigin {
                    scope: InventoryScope::Group,
                    id: Some(origin.id.clone()),
                    name: origin.name,
                },
            )
        })
        .collect()
}

// === P4.D377 === the shared folder layer.
impl ItemFolderLocation for InventoryLocation {
    type Error = InventoryError;

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
        InventoryLocation::list_folders(self, mount)
    }
    fn resolve_folder(&self, mount: &Connection, requested: &str) -> Result<String, Self::Error> {
        InventoryLocation::resolve_folder(self, mount, requested)
    }
    fn create_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<InventoryFolder, Self::Error> {
        InventoryLocation::create_folder(self, main, mount, path)
    }
    fn rename_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
        new_name: &str,
    ) -> Result<InventoryFolder, Self::Error> {
        InventoryLocation::rename_folder(self, main, mount, path, new_name)
    }
    fn delete_folder(
        &self,
        main: &Connection,
        mount: &Connection,
        path: &str,
    ) -> Result<DeletedFolder, Self::Error> {
        InventoryLocation::delete_folder(self, main, mount, path)
    }
}

impl ItemFilingLocation for InventoryLocation {
    type Item = InventoryRecord;

    fn refile_items(
        &self,
        main: &Connection,
        mount: &Connection,
        item_ids: &[String],
        folder: &str,
    ) -> Result<Vec<InventoryRecord>, InventoryError> {
        let patches: Vec<InventoryRecordPatch> = item_ids
            .iter()
            .map(|id| {
                let mut patch = serde_json::Map::new();
                patch.insert("folder".into(), Value::String(folder.to_string()));
                InventoryRecordPatch {
                    id: id.clone(),
                    patch,
                    remove: Vec::new(),
                }
            })
            .collect();
        self.update_many(main, mount, &patches, true)
    }
}
// === P4.D377 ===

#[cfg(test)]
mod publish_tests {
    //! P4.D350 R-1 (v4 `0276d10d9`, `chokepoints.test.ts`'s publish rows): a
    //! landed write through a location announces `inventory` once; a batch
    //! that matched nothing announces nothing; a folder write announces; the
    //! one carried-set writer announces `chats/<id>`, and a drop announces
    //! only the chats it changed. Driven over a real provisioned instance (its
    //! Quilltap General store) with the bus armed on the writer thread.
    use super::*;
    use crate::db::runtime::{Db, DbPaths};
    use crate::realtime::publish_sites::HintCapture;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const ITEM: &str = "a0000001-0000-4000-8000-0000000000aa";

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

    /// Run `op` over General's location inside one write; answers its debug
    /// text so a failure reads in the assertion.
    async fn in_general<F>(db: &Db, op: F) -> String
    where
        F: FnOnce(&Connection, &Connection, &InventoryLocation) -> String + Send + 'static,
    {
        db.write(move |ws| {
            let main = ws.main().connection();
            let mount = ws.mount_index().expect("mount index").connection();
            let loc = resolve_inventory_location(
                main,
                mount,
                InventoryScope::General,
                None,
                "u-1",
                ResolveInventoryLocationOptions::default(),
            )?
            .expect("a provisioned instance has Quilltap General");
            Ok(op(main, mount, &loc))
        })
        .await
        .unwrap()
    }

    fn inventory() -> (String, Option<String>) {
        ("inventory".to_string(), None)
    }

    #[tokio::test]
    async fn a_landed_location_write_announces_inventory_and_a_miss_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, db) = db();
        cap.arm_writer_thread(&db).await;

        let created = in_general(&db, |main, mount, loc| {
            let record = InventoryRecord(
                json!({ "id": ITEM, "name": "lantern" })
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            format!(
                "{:?}",
                loc.create(main, mount, &record).map(|r| r.id().to_string())
            )
        })
        .await;
        assert_eq!(created, format!("Ok({ITEM:?})"));
        assert_eq!(cap.drain_expecting(1).await, vec![inventory()]);

        // A read announces nothing; an update or a delete that matched nothing
        // changed nothing and announces nothing.
        let missed = in_general(&db, |main, mount, loc| {
            let read = loc.read_items(mount, true).map(|r| r.len());
            let updated = loc
                .update_many(
                    main,
                    mount,
                    &[InventoryRecordPatch {
                        id: "nope".into(),
                        ..Default::default()
                    }],
                    false,
                )
                .map(|u| u.len());
            let deleted = loc
                .delete(main, mount, &["nope".to_string()], |_| Vec::new())
                .map(|d| d.len());
            format!("{read:?} {updated:?} {deleted:?}")
        })
        .await;
        assert_eq!(missed, "Ok(1) Ok(0) Ok(0)");
        assert_eq!(cap.drain().await, Vec::<(String, Option<String>)>::new());

        // A folder operation announces too.
        let folder = in_general(&db, |main, mount, loc| {
            format!(
                "{:?}",
                loc.create_folder(main, mount, "Shelf").map(|f| f.path)
            )
        })
        .await;
        assert_eq!(folder, "Ok(\"Shelf\")");
        assert_eq!(cap.drain_expecting(1).await, vec![inventory()]);
    }

    #[tokio::test]
    async fn the_carried_set_writer_and_a_drop_announce_the_chat() {
        let mut cap = HintCapture::start();
        let (_dir, db) = db();
        cap.arm_writer_thread(&db).await;

        db.write(|ws| {
            crate::inventory::equip::write_carried_set(
                ws.main().connection(),
                "chat-1",
                "c-1",
                Some(&[]),
            );
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(
            cap.drain_expecting(1).await,
            vec![("chats".to_string(), Some("chat-1".to_string()))]
        );

        // A drop that changes no chat announces nothing.
        let changed = db
            .write(|ws| {
                Ok(crate::inventory::equip::drop_carried_everywhere(
                    ws.main().connection(),
                    "c-1",
                    ITEM,
                    &["chat-1".to_string()],
                ))
            })
            .await
            .unwrap();
        assert_eq!(changed, 0);
        assert_eq!(cap.drain().await, Vec::<(String, Option<String>)>::new());
    }
}
