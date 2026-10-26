//! Inventory dependents — every instance of an archetype, instance-wide (v4
//! `lib/inventory/dependents.ts`, `4fbdad186` + `e1872c94f`; P4.D324).
//!
//! An archetype may live in any tier and its instances in any other, so the
//! question "what derives from this?" is answered across **every** mount, not
//! a pool. Deletion is rare, so there is no dependency table: one
//! `doc_mount_documents` query reads every `Inventory/**/*.json` in the
//! instance, and the records are parsed and filtered. The same read answers
//! the pool's fallback for an archetype outside the tiers it can see —
//! through [`find_records_anywhere`], which asks the index only for the files
//! that MENTION a wanted id (a substring hit — callers re-parse and match the
//! id exactly).

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use super::errors::StrandedKit;
use super::folders::{is_inventory_item_document_path, INVENTORY_FOLDER};
use super::types::{InventoryFields, InventoryRecord, InventoryScope};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_points::{find_all_full_json, DocMountPointsRepository};
use crate::db::groups::GroupsRepository;
use crate::db::inventory_readers::{parse_inventory_item_file, InventoryDoc};
use crate::db::projects::ProjectsRepository;
use crate::db::{characters_read, fallback, instance_settings, DbError};

/// A record and the mount it was read from (v4 `MountedInventoryRecord`).
#[derive(Debug, Clone)]
pub struct MountedInventoryRecord {
    pub record: InventoryRecord,
    pub mount_point_id: String,
}

/// Which tier a mount is (v4 `InventoryScope | 'unknown'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountScope {
    Character,
    Group,
    Project,
    General,
    Unknown,
}

impl MountScope {
    pub fn as_str(self) -> &'static str {
        match self {
            MountScope::Character => "character",
            MountScope::Group => "group",
            MountScope::Project => "project",
            MountScope::General => "general",
            MountScope::Unknown => "unknown",
        }
    }

    /// The tier, `None` for `unknown`.
    pub fn inventory_scope(self) -> Option<InventoryScope> {
        match self {
            MountScope::Character => Some(InventoryScope::Character),
            MountScope::Group => Some(InventoryScope::Group),
            MountScope::Project => Some(InventoryScope::Project),
            MountScope::General => Some(InventoryScope::General),
            MountScope::Unknown => None,
        }
    }
}

impl Serialize for MountScope {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

/// Who owns a mount (v4 `describeMount`'s answer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountDescription {
    pub scope: MountScope,
    pub owner_id: Option<String>,
    pub owner_name: Option<String>,
    pub owner_archived: bool,
}

/// One instance of an archetype, wherever it lives (v4 `InventoryDependent`
/// — the key order of its literal: `id, name, title, mountPointId`, then the
/// mount description's `scope, ownerId, ownerName, ownerArchived`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryDependent {
    pub id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub mount_point_id: String,
    pub scope: MountScope,
    pub owner_id: Option<String>,
    pub owner_name: Option<String>,
    pub owner_archived: bool,
}

/// Every Inventory record in every mount, unresolved (v4
/// `readAllInventoryRecords`): ONE folder read over every mount id, quiet
/// parse. DEBUG `[InventoryDependents] Read every Inventory record`.
pub fn read_all_inventory_records(
    mount: &Connection,
) -> Result<Vec<MountedInventoryRecord>, DbError> {
    let mounts = fallback::find_all_or_empty("doc_mount_points", || find_all_full_json(mount));
    if mounts.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = mounts
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let docs = DocMountDocumentsRepository::new(mount)
        .find_many_by_mount_points_in_folder_opts_or_empty(&ids, INVENTORY_FOLDER, ".json", true)?;
    let mut out = Vec::new();
    for doc in &docs {
        if !is_inventory_item_document_path(&doc.relative_path) {
            continue;
        }
        if let Some(record) = parse_inventory_item_file(&InventoryDoc::of(doc), true) {
            out.push(MountedInventoryRecord {
                record,
                mount_point_id: doc.mount_point_id.clone(),
            });
        }
    }
    tracing::debug!(
        mountCount = mounts.len(),
        recordCount = out.len(),
        op = "read-all",
        context = "inventory",
        "[InventoryDependents] Read every Inventory record"
    );
    Ok(out)
}

/// Find records by id anywhere in the instance (v4 `findRecordsAnywhere` —
/// the pool's parent fallback): only the files that mention a wanted id are
/// asked for, the object paths sorted by `localeCompare` (stable), parsed
/// quietly, the FIRST match per wanted id kept. Answers `(id, record)` in
/// first-found order. DEBUG `[InventoryDependents] Found records by id across
/// the instance`.
pub fn find_records_anywhere(
    mount: &Connection,
    ids: &[String],
) -> Vec<(String, MountedInventoryRecord)> {
    let mut wanted: Vec<String> = Vec::new();
    for id in ids {
        if !wanted.contains(id) {
            wanted.push(id.clone());
        }
    }
    let mut found: Vec<(String, MountedInventoryRecord)> = Vec::new();
    if wanted.is_empty() {
        return found;
    }
    let docs = DocMountDocumentsRepository::new(mount).find_many_in_folder_containing_or_empty(
        INVENTORY_FOLDER,
        ".json",
        &wanted,
    );
    let mut candidates: Vec<_> = docs
        .iter()
        .filter(|d| is_inventory_item_document_path(&d.relative_path))
        .collect();
    candidates.sort_by(|a, b| crate::collation::locale_compare(&a.relative_path, &b.relative_path));
    for doc in &candidates {
        let folder_doc = doc.to_folder_doc();
        if let Some(record) = parse_inventory_item_file(&InventoryDoc::of(&folder_doc), true) {
            let id = record.id().to_string();
            if wanted.contains(&id) && !found.iter().any(|(f, _)| *f == id) {
                found.push((
                    id,
                    MountedInventoryRecord {
                        record,
                        mount_point_id: doc.mount_point_id.clone(),
                    },
                ));
            }
        }
    }
    tracing::debug!(
        wanted = wanted.len(),
        candidates = candidates.len(),
        found = found.len(),
        op = "find-anywhere",
        context = "inventory",
        "[InventoryDependents] Found records by id across the instance"
    );
    found
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Who owns a mount: a character vault, a group or project store, or General
/// (v4 `describeMount`). Precedence: General, then the character whose vault
/// it is, then the first project / group whose official store it is, else
/// `unknown` named by the mount row.
pub fn describe_mount(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
    general_mount_point_id: Option<&str>,
) -> Result<MountDescription, DbError> {
    if Some(mount_point_id) == general_mount_point_id {
        return Ok(MountDescription {
            scope: MountScope::General,
            owner_id: None,
            owner_name: Some("Quilltap General".to_string()),
            owner_archived: false,
        });
    }
    if let Some(character) =
        characters_read::find_by_document_mount_point_id_raw(main, mount_point_id)
    {
        return Ok(MountDescription {
            scope: MountScope::Character,
            owner_id: str_field(&character, "id"),
            owner_name: str_field(&character, "name"),
            owner_archived: crate::inventory::types::js_truthy(character.get("archivedAt")),
        });
    }
    let official = |rows: Vec<Value>| {
        rows.into_iter()
            .find(|r| r.get("officialMountPointId").and_then(Value::as_str) == Some(mount_point_id))
    };
    let projects = ProjectsRepository::new(main, mount)
        .find_all()
        .map_err(|e| e.into_db())?;
    if let Some(project) = official(projects) {
        return Ok(MountDescription {
            scope: MountScope::Project,
            owner_id: str_field(&project, "id"),
            owner_name: str_field(&project, "name"),
            owner_archived: false,
        });
    }
    let groups = GroupsRepository::new(main, mount)
        .find_all()
        .map_err(|e| e.into_db())?;
    if let Some(group) = official(groups) {
        return Ok(MountDescription {
            scope: MountScope::Group,
            owner_id: str_field(&group, "id"),
            owner_name: str_field(&group, "name"),
            owner_archived: false,
        });
    }
    let name = fallback::find_by_id_or_none("doc_mount_points", mount_point_id, || {
        DocMountPointsRepository::new(mount).find_id_and_name_by_id(mount_point_id)
    })
    .map(|(_, name)| name);
    Ok(MountDescription {
        scope: MountScope::Unknown,
        owner_id: None,
        owner_name: name,
        owner_archived: false,
    })
}

/// Which tier a mount is, or `None` when it belongs to no Inventory owner
/// (v4 `mountScopeOf`).
pub fn mount_scope_of(
    main: &Connection,
    mount: &Connection,
    mount_point_id: &str,
) -> Result<Option<InventoryScope>, DbError> {
    let general = instance_settings::get_general_mount_point_id(main)?;
    Ok(
        describe_mount(main, mount, mount_point_id, general.as_deref())?
            .scope
            .inventory_scope(),
    )
}

/// Whether an object living at `target` is within reach of a kit kept at
/// `kit_mount_point_id` (v4 `isReachableFromKitTier`): its own tier, Quilltap
/// General, and for a character's vault the groups they belong to and any
/// project (a project member is reached in that project's chats).
pub fn is_reachable_from_kit_tier(
    main: &Connection,
    mount: &Connection,
    kit_mount_point_id: &str,
    target_mount_point_id: &str,
    target_scope: InventoryScope,
) -> Result<bool, DbError> {
    if target_mount_point_id == kit_mount_point_id || target_scope == InventoryScope::General {
        return Ok(true);
    }
    let general = instance_settings::get_general_mount_point_id(main)?;
    let owner = describe_mount(main, mount, kit_mount_point_id, general.as_deref())?;
    let (MountScope::Character, Some(owner_id)) = (owner.scope, owner.owner_id) else {
        return Ok(false);
    };
    if target_scope == InventoryScope::Project {
        return Ok(true);
    }
    if target_scope != InventoryScope::Group {
        return Ok(false);
    }
    Ok(
        super::location::group_locations_for_character(main, mount, &owner_id)
            .iter()
            .any(|loc| loc.mount_point_id == target_mount_point_id),
    )
}

/// The non-archived kits, anywhere, that name `item_id` and could not reach
/// it once it lives at `target` — what a move would strand (v4
/// `findKitsStrandedByMove`). DEBUG `[InventoryDependents] Checked kits a
/// move would strand`.
pub fn find_kits_stranded_by_move(
    main: &Connection,
    mount: &Connection,
    item_id: &str,
    target_mount_point_id: &str,
    target_scope: InventoryScope,
) -> Result<Vec<StrandedKit>, DbError> {
    let docs = DocMountDocumentsRepository::new(mount).find_many_in_folder_containing_or_empty(
        INVENTORY_FOLDER,
        ".json",
        &[item_id.to_string()],
    );
    let general = instance_settings::get_general_mount_point_id(main)?;
    let mut out = Vec::new();
    for doc in &docs {
        if !is_inventory_item_document_path(&doc.relative_path) {
            continue;
        }
        let folder_doc = doc.to_folder_doc();
        let Some(kit) = parse_inventory_item_file(&InventoryDoc::of(&folder_doc), true) else {
            continue;
        };
        if !kit.is_kit() || kit.is_archived() || !kit.item_ids().iter().any(|i| i == item_id) {
            continue;
        }
        if is_reachable_from_kit_tier(
            main,
            mount,
            &doc.mount_point_id,
            target_mount_point_id,
            target_scope,
        )? {
            continue;
        }
        let owner = describe_mount(main, mount, &doc.mount_point_id, general.as_deref())?;
        // `kit.title || kit.name || null`.
        let name = [kit.title(), kit.str_of("name")]
            .into_iter()
            .flatten()
            .find(|s| !s.is_empty())
            .map(str::to_string);
        out.push(StrandedKit {
            id: kit.id().to_string(),
            name,
            owner_name: owner.owner_name,
            mount_point_id: doc.mount_point_id.clone(),
        });
    }
    tracing::debug!(
        itemId = %item_id,
        targetMountPointId = %target_mount_point_id,
        targetScope = target_scope.as_str(),
        candidates = docs.len(),
        stranded = out.len(),
        op = "kits-stranded",
        context = "inventory",
        "[InventoryDependents] Checked kits a move would strand"
    );
    Ok(out)
}

/// Every instance of `archetype_id`, in every mount, tagged with its holder
/// (v4 `findDependents`; the mount description memoised per mount). DEBUG
/// `[InventoryDependents] Found dependents`.
pub fn find_dependents(
    main: &Connection,
    mount: &Connection,
    archetype_id: &str,
) -> Result<Vec<InventoryDependent>, DbError> {
    let dependents: Vec<MountedInventoryRecord> = read_all_inventory_records(mount)?
        .into_iter()
        .filter(|m| m.record.str_of("parentId") == Some(archetype_id))
        .collect();
    let general = instance_settings::get_general_mount_point_id(main)?;
    let mut owners: Vec<(String, MountDescription)> = Vec::new();
    let mut out = Vec::new();
    for MountedInventoryRecord {
        record,
        mount_point_id,
    } in dependents
    {
        let owner = match owners.iter().find(|(mp, _)| *mp == mount_point_id) {
            Some((_, d)) => d.clone(),
            None => {
                let d = describe_mount(main, mount, &mount_point_id, general.as_deref())?;
                owners.push((mount_point_id.clone(), d.clone()));
                d
            }
        };
        out.push(InventoryDependent {
            id: record.id().to_string(),
            name: record.str_of("name").map(str::to_string),
            title: record.title().map(str::to_string),
            mount_point_id,
            scope: owner.scope,
            owner_id: owner.owner_id,
            owner_name: owner.owner_name,
            owner_archived: owner.owner_archived,
        });
    }
    tracing::debug!(
        itemId = %archetype_id,
        count = out.len(),
        archivedHolders = out.iter().filter(|d| d.owner_archived).count(),
        op = "dependents",
        context = "inventory",
        "[InventoryDependents] Found dependents"
    );
    Ok(out)
}
