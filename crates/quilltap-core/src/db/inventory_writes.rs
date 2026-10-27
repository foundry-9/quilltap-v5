//! Inventory writes — the one place an Inventory object reaches disk (v4
//! `lib/database/repositories/vault-overlay/inventory-writes.ts`,
//! `4fbdad186`; P4.D324).
//!
//! Every tier's objects are `.json` files under an `Inventory/` folder in some
//! mount (a character's vault, a group store, a project store, Quilltap
//! General). Each mutation reads the folder's records, applies the change in
//! memory and re-projects the whole folder through
//! [`project_array_into_vault_folder`] (`.json`, recursive, `images/`
//! excluded, sweeping only files [`is_inventory_item_document`] owns). The
//! folder operations are the wardrobe's, through the shared
//! [`super::item_folder_ops`] descriptor ([`InventoryTree`]).
//!
//! Records are stored as read: an instance's file stays sparse. Callers reach
//! this through `inventory::location::resolve_inventory_location`.
//!
//! **NO-PORT: `runSerialized`** — v5's writer task serialises every write.
//! **`with_inventory_writes_linted`** (P4.D340, R-9) — v4 runs these writes
//! inside `withInventoryWritesLinted` so the document-store hooks skip them
//! (the mutations lint for themselves). v5 passes the scope as an explicit
//! argument: the projection writes through the REPOSITORY methods, which
//! carry no hook at all (the same outcome as v4's linted scope); the one
//! write here that reaches a hook is the folder RENAME's
//! `move_database_folder`, which takes the scope this closure hands out.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::inventory::lint_scope::TitleLintScope;

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::inventory_readers::{
    has_stable_inventory_item_id, inventory_file_name_for, is_inventory_item_document,
    read_inventory_records, scan_inventory_folder,
};
use super::item_folder_ops::{self, DeletedFolder, FiledItem, ItemFolderTree, VaultFolderMount};
use super::vault_wardrobe_write::{project_array_into_vault_folder, ProjectArrayOptions};
use super::DbError;
use crate::clock;
use crate::inventory::folders::{
    is_safe_inventory_folder_path, normalize_inventory_folder_path_or_root, InventoryFolder,
    InventoryFolderError, INVENTORY_FOLDER, INVENTORY_FOLDER_RULES,
    INVENTORY_RESERVED_FOLDER_SEGMENTS,
};
use crate::inventory::item_folders::ItemFolderRules;
use crate::inventory::types::{
    js_json_stringify_pretty, InventoryFields, InventoryRecord, INVENTORY_FILE_KEYS,
};

/// An Inventory tier's folder on disk (v4 `InventoryMount = VaultFolderMount`).
pub type InventoryMount = VaultFolderMount;

/// A write's failure: a folder rule / missing folder / conflict / foreign id,
/// or storage.
#[derive(Debug)]
pub enum InventoryWriteError {
    Folder(InventoryFolderError),
    Db(DbError),
}

impl From<DbError> for InventoryWriteError {
    fn from(e: DbError) -> Self {
        InventoryWriteError::Db(e)
    }
}

impl From<InventoryFolderError> for InventoryWriteError {
    fn from(e: InventoryFolderError) -> Self {
        InventoryWriteError::Folder(e)
    }
}

impl std::fmt::Display for InventoryWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InventoryWriteError::Folder(e) => e.fmt(f),
            InventoryWriteError::Db(e) => f.write_str(&super::fallback::error_text(e)),
        }
    }
}

impl From<InventoryWriteError> for crate::inventory::errors::InventoryError {
    fn from(e: InventoryWriteError) -> Self {
        match e {
            InventoryWriteError::Folder(f) => crate::inventory::errors::InventoryError::Folder(f),
            InventoryWriteError::Db(d) => d.into(),
        }
    }
}

/// Resolve the Inventory folder's mount for a character id, or Quilltap
/// General for `None` (v4 `resolveInventoryMount` — a delegate over
/// `resolveVaultFolderMount`; [`DbError::CharacterArchived`] for the
/// tombstone).
pub fn resolve_inventory_mount(
    main: &Connection,
    character_id: Option<&str>,
) -> Result<Option<InventoryMount>, DbError> {
    item_folder_ops::resolve_vault_folder_mount(main, character_id)
}

/// Every record in the folder, archived included (v4 `readMountInventory`).
pub fn read_mount_inventory(
    mount: &Connection,
    m: &InventoryMount,
) -> Result<Vec<InventoryRecord>, DbError> {
    read_inventory_records(mount, &m.mount_point_id)
}

/// The file a record serialises to (v4 `serializeInventoryRecord`): the known
/// keys in `INVENTORY_FILE_KEYS` order (`folder` and `fileStem` left out), an
/// absent key skipped, an explicit `null` WRITTEN, `JSON.stringify(out, null,
/// 2) + '\n'`.
pub fn serialize_inventory_record(record: &InventoryRecord) -> String {
    let mut out = Map::new();
    for key in INVENTORY_FILE_KEYS {
        if let Some(v) = record.get(key) {
            out.insert(key.to_string(), v.clone());
        }
    }
    format!("{}\n", js_json_stringify_pretty(&Value::Object(out)))
}

/// The folder a record is written to (v4 `safeFolder`, the wardrobe's twin).
fn safe_folder(folder: Option<&str>) -> String {
    let Some(folder) = folder.filter(|f| !f.is_empty()) else {
        return String::new();
    };
    if is_safe_inventory_folder_path(folder) {
        return folder.to_string();
    }
    let normalised = normalize_inventory_folder_path_or_root(Some(folder));
    tracing::debug!(
        target: "quilltap::db",
        folder = %folder,
        normalised = %normalised,
        context = "inventory",
        "Inventory write normalised an incoming folder"
    );
    normalised
}

/// Re-project the folder from an authoritative record list (v4
/// `projectMountInventory`). A file the reader skipped as a duplicate id is
/// not owned, so the sweep leaves it in place, and its leaf name is claimed
/// so no projected object is written over it.
pub fn project_mount_inventory(
    mount: &Connection,
    m: &InventoryMount,
    records: &[InventoryRecord],
) -> Result<(), DbError> {
    let duplicates = scan_inventory_folder(mount, &m.mount_point_id)?.duplicate_paths;
    if !duplicates.is_empty() {
        tracing::debug!(
            target: "quilltap::db",
            mountPointId = %m.mount_point_id,
            pathsJson = %serde_json::to_string(&duplicates).unwrap_or_default(),
            context = "inventory",
            "Inventory projection leaving duplicate-id files alone"
        );
    }
    let leaves: Vec<String> = duplicates
        .iter()
        .map(|p| match p.rfind('/') {
            Some(i) => p[i + 1..].to_string(),
            None => p.clone(),
        })
        .collect();
    let leaf_refs: Vec<&str> = leaves.iter().map(String::as_str).collect();
    let owns = |path: &str, content: Option<&str>| {
        !duplicates.iter().any(|d| d == path) && is_inventory_item_document(path, content)
    };
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    project_array_into_vault_folder(
        &links,
        &docs,
        &m.mount_point_id,
        INVENTORY_FOLDER,
        records,
        |record| {
            (
                inventory_file_name_for(record),
                serialize_inventory_record(record),
            )
        },
        m.scope_id(),
        ProjectArrayOptions {
            preserve_file_names: &leaf_refs,
            recursive: Some(INVENTORY_RESERVED_FOLDER_SEGMENTS),
            owns_file: Some(&owns),
            ext: ".json",
        },
    )
}

/// Create a record in the folder (v4 `createInventoryInMount`): `{...record,
/// folder: safeFolder(record.folder)}` appended, one projection. DEBUG
/// `Inventory object created in folder`.
pub fn create_inventory_in_mount(
    mount: &Connection,
    m: &InventoryMount,
    record: &InventoryRecord,
) -> Result<InventoryRecord, DbError> {
    let mut current = read_mount_inventory(mount, m)?;
    let mut stored = record.clone();
    stored.set(
        "folder",
        Value::String(safe_folder(record.str_of("folder"))),
    );
    current.push(stored.clone());
    project_mount_inventory(mount, m, &current)?;
    tracing::debug!(
        target: "quilltap::db",
        scope = m.scope.as_str(),
        mountPointId = %m.mount_point_id,
        itemId = %stored.id(),
        folder = %stored.folder(),
        op = "create",
        context = "inventory",
        "Inventory object created in folder"
    );
    Ok(stored)
}

/// One record's change in an [`update_inventory_many_in_mount`] batch (v4
/// `InventoryRecordPatch`): fields to set (keys of an `InventoryRecord`, in
/// the order the caller built them), and stored keys to remove (an
/// instance's override cleared).
#[derive(Debug, Clone, Default)]
pub struct InventoryRecordPatch {
    pub id: String,
    pub patch: Map<String, Value>,
    pub remove: Vec<String>,
}

/// Patch several records with one read and one projection (v4
/// `updateInventoryManyInMount`); the updated records in patch order. A
/// missing id is skipped, or with `require_all` refuses the batch (`Not in
/// this inventory …`, deduped) before anything is written. Nothing updated →
/// `[]` with NO projection. DEBUG `Inventory objects updated in folder`.
pub fn update_inventory_many_in_mount(
    mount: &Connection,
    m: &InventoryMount,
    patches: &[InventoryRecordPatch],
    require_all: bool,
) -> Result<Vec<InventoryRecord>, InventoryWriteError> {
    if patches.is_empty() {
        return Ok(Vec::new());
    }
    update_many_unserialized(mount, m, patches, require_all)
}

fn update_many_unserialized(
    mount: &Connection,
    m: &InventoryMount,
    patches: &[InventoryRecordPatch],
    require_all: bool,
) -> Result<Vec<InventoryRecord>, InventoryWriteError> {
    let current = read_mount_inventory(mount, m)?;
    let index_of = |id: &str| current.iter().position(|r| r.id() == id);
    if require_all {
        let mut missing: Vec<String> = Vec::new();
        for p in patches {
            if index_of(&p.id).is_none() && !missing.contains(&p.id) {
                missing.push(p.id.clone());
            }
        }
        if !missing.is_empty() {
            return Err(InventoryFolderError::NotInLocation(missing).into());
        }
    }
    let mut next = current.clone();
    let now = clock::now_iso();
    let mut updated: Vec<usize> = Vec::new();
    for InventoryRecordPatch { id, patch, remove } in patches {
        let Some(idx) = index_of(id) else {
            continue;
        };
        let before = next[idx].clone();
        // `{...before, ...patch, ...(folder && {folder: safeFolder}), id,
        //   createdAt: before.createdAt, updatedAt: now}`, then `remove`.
        let mut merged = before.clone();
        for (k, v) in patch {
            merged.set(k, v.clone());
        }
        if let Some(folder) = patch.get("folder") {
            merged.set("folder", Value::String(safe_folder(folder.as_str())));
        }
        merged.set("id", Value::String(before.id().to_string()));
        match before.get("createdAt") {
            Some(c) => merged.set("createdAt", c.clone()),
            None => merged.remove("createdAt"),
        }
        merged.set("updatedAt", Value::String(now.clone()));
        for key in remove {
            merged.remove(key);
        }
        next[idx] = merged;
        updated.push(idx);
    }
    if updated.is_empty() {
        return Ok(Vec::new());
    }
    project_mount_inventory(mount, m, &next)?;
    let ids: Vec<&str> = updated.iter().map(|i| next[*i].id()).collect();
    tracing::debug!(
        target: "quilltap::db",
        scope = m.scope.as_str(),
        mountPointId = %m.mount_point_id,
        itemIdsJson = %serde_json::to_string(&ids).unwrap_or_default(),
        count = ids.len(),
        op = "update",
        context = "inventory",
        "Inventory objects updated in folder"
    );
    Ok(updated.into_iter().map(|i| next[i].clone()).collect())
}

/// Delete records from the folder, and in the same projection apply any
/// follow-on patches (a deleted kit member dropped from the kits that named
/// it) (v4 `deleteInventoryInMount`). Returns the ids actually deleted (in
/// FOLDER order); `[]` short-circuits with no projection. DEBUG `Inventory
/// objects deleted from folder`.
pub fn delete_inventory_in_mount(
    mount: &Connection,
    m: &InventoryMount,
    ids: &[String],
    follow_on: impl FnOnce(&[InventoryRecord]) -> Vec<InventoryRecordPatch>,
) -> Result<Vec<String>, DbError> {
    let current = read_mount_inventory(mount, m)?;
    let doomed = |r: &InventoryRecord| ids.iter().any(|i| i == r.id());
    let deleted: Vec<String> = current
        .iter()
        .filter(|r| doomed(r))
        .map(|r| r.id().to_string())
        .collect();
    if deleted.is_empty() {
        return Ok(Vec::new());
    }
    let mut remaining: Vec<InventoryRecord> = current.into_iter().filter(|r| !doomed(r)).collect();
    let patches = follow_on(&remaining);
    if !patches.is_empty() {
        let now = clock::now_iso();
        remaining = remaining
            .into_iter()
            .map(
                |record| match patches.iter().find(|p| p.id == record.id()) {
                    // `{...record, ...p.patch, id: record.id, updatedAt: now}` —
                    // NO `remove` here (v4's).
                    Some(p) => {
                        let mut merged = record.clone();
                        for (k, v) in &p.patch {
                            merged.set(k, v.clone());
                        }
                        merged.set("id", Value::String(record.id().to_string()));
                        merged.set("updatedAt", Value::String(now.clone()));
                        merged
                    }
                    None => record,
                },
            )
            .collect();
    }
    project_mount_inventory(mount, m, &remaining)?;
    tracing::debug!(
        target: "quilltap::db",
        scope = m.scope.as_str(),
        mountPointId = %m.mount_point_id,
        itemIdsJson = %serde_json::to_string(&deleted).unwrap_or_default(),
        followOnPatches = patches.len(),
        op = "delete",
        context = "inventory",
        "Inventory objects deleted from folder"
    );
    Ok(deleted)
}

// ============================================================================
// Folders — the shared folder operations over the Inventory's tree
// ============================================================================

/// The Inventory's folder tree, as the shared folder operations see it (v4
/// `INVENTORY_TREE`): `.json` objects, context `inventory`, the readers'
/// predicates, `readMountInventory`, `projectMountInventory`, and a refile
/// through `updateManyUnserialized` (which RE-READS the folder — v4's).
pub struct InventoryTree;

/// v4 `INVENTORY_TREE`.
pub const INVENTORY_TREE: InventoryTree = InventoryTree;

impl ItemFolderTree for InventoryTree {
    type Item = InventoryRecord;
    type Error = InventoryWriteError;

    fn rules(&self) -> &'static ItemFolderRules {
        &INVENTORY_FOLDER_RULES
    }
    fn ext(&self) -> &'static str {
        ".json"
    }
    fn context(&self) -> &'static str {
        "inventory"
    }
    fn is_item_document(&self, relative_path: &str, content: Option<&str>) -> bool {
        is_inventory_item_document(relative_path, content)
    }
    fn has_stable_id(&self, content: Option<&str>) -> bool {
        has_stable_inventory_item_id(content)
    }
    fn filed(&self, item: &InventoryRecord) -> FiledItem {
        FiledItem {
            id: item.id().to_string(),
            folder: item.folder().to_string(),
            archived: item.is_archived(),
        }
    }
    fn read(
        &self,
        mount: &Connection,
        m: &InventoryMount,
    ) -> Result<Vec<InventoryRecord>, DbError> {
        read_mount_inventory(mount, m)
    }
    fn reproject(
        &self,
        mount: &Connection,
        m: &InventoryMount,
        items: &[InventoryRecord],
    ) -> Result<(), DbError> {
        project_mount_inventory(mount, m, items)
    }
    fn refile(
        &self,
        _main: &Connection,
        mount: &Connection,
        m: &InventoryMount,
        _current: &[InventoryRecord],
        patches: &[(String, String)],
    ) -> Result<(), InventoryWriteError> {
        let patches: Vec<InventoryRecordPatch> = patches
            .iter()
            .map(|(id, folder)| {
                let mut patch = Map::new();
                patch.insert("folder".into(), Value::String(folder.clone()));
                InventoryRecordPatch {
                    id: id.clone(),
                    patch,
                    remove: Vec::new(),
                }
            })
            .collect();
        update_many_unserialized(mount, m, &patches, false).map(|_| ())
    }
    fn path_error(&self, message: String) -> InventoryWriteError {
        InventoryFolderError::Path(message).into()
    }
    fn not_found(&self, folder: &str) -> InventoryWriteError {
        InventoryFolderError::NotFound(folder.to_string()).into()
    }
    fn conflict(&self, folder: &str, message: Option<String>) -> InventoryWriteError {
        match message {
            Some(m) => InventoryFolderError::Conflict(m).into(),
            None => InventoryFolderError::conflict_default(folder).into(),
        }
    }
}

/// Every folder under the tier's `Inventory/`, with direct object counts.
pub fn list_inventory_folders_in_mount(
    mount: &Connection,
    m: &InventoryMount,
) -> Result<Vec<InventoryFolder>, DbError> {
    item_folder_ops::list_folders_in(&INVENTORY_TREE, mount, m)
}

/// Every folder path under the tier's `Inventory/`, from the store's folder
/// rows (a fallback read).
pub fn list_inventory_folder_paths_in_mount(mount: &Connection, m: &InventoryMount) -> Vec<String> {
    item_folder_ops::list_folder_paths_in(&INVENTORY_TREE, mount, m)
}

/// Create a folder (normalised path); an existing one in any casing is
/// answered.
pub fn create_inventory_folder_in_mount(
    mount: &Connection,
    m: &InventoryMount,
    folder: &str,
) -> Result<InventoryFolder, InventoryWriteError> {
    item_folder_ops::create_folder_in(&INVENTORY_TREE, mount, m, folder)
}

/// Run `f` with its document writes marked as already linted by the
/// Inventory mutations (v4 `withInventoryWritesLinted`).
pub fn with_inventory_writes_linted<T>(f: impl FnOnce(TitleLintScope<'static>) -> T) -> T {
    f(TitleLintScope::Linted)
}

/// Rename a folder in place.
pub fn rename_inventory_folder_in_mount(
    mount: &Connection,
    m: &InventoryMount,
    folder: &str,
    new_name: &str,
) -> Result<InventoryFolder, InventoryWriteError> {
    // v4 `runSerialized` wraps the folder writers in `withInventoryWritesLinted`.
    with_inventory_writes_linted(|linted| {
        item_folder_ops::rename_folder_in(&INVENTORY_TREE, mount, m, folder, new_name, linted)
    })
}

/// Delete a folder, sending its objects and subfolders up to its parent.
pub fn delete_inventory_folder_in_mount(
    main: &Connection,
    mount: &Connection,
    m: &InventoryMount,
    folder: &str,
) -> Result<DeletedFolder, InventoryWriteError> {
    item_folder_ops::delete_folder_in(&INVENTORY_TREE, main, mount, m, folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serialize_keeps_file_key_order_and_nulls() {
        let r = InventoryRecord(
            json!({
                "folder": "x", "fileStem": "s", "updatedAt": "u", "title": null,
                "name": "Lamp", "id": "i", "metadata": {"w": 1.0, "2": 2}, "createdAt": "c"
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        assert_eq!(
            serialize_inventory_record(&r),
            "{\n  \"id\": \"i\",\n  \"name\": \"Lamp\",\n  \"title\": null,\n  \"metadata\": {\n    \"2\": 2,\n    \"w\": 1\n  },\n  \"createdAt\": \"c\",\n  \"updatedAt\": \"u\"\n}\n"
        );
    }
}
