//! Wardrobe writes — the one place a wardrobe item reaches disk (v4
//! `lib/database/repositories/vault-overlay/wardrobe-writes.ts` at
//! `4a38222e6`; P4.D279, keystone C1 §2 / §4).
//!
//! Every wardrobe tier is a `Wardrobe/` folder in some mount: a character's
//! vault, Quilltap General, a project store or a group store. Each mutation
//! reads the folder's current items, applies the change in memory, and
//! re-projects the whole folder via [`project_vault_wardrobe`] (which dedupes
//! filenames per folder, renames on title change, moves a filed item to its
//! new path, and sweeps removed item files). The `wardrobe_items` DB table is
//! gone; nothing here writes a row.
//!
//! Callers address a folder through a [`WardrobeMount`]. New code gets one
//! from [`super::wardrobe_location::resolve_wardrobe_location`], which also
//! owns the archived-character tombstone and the ownership gate;
//! [`resolve_wardrobe_mount`] is the character-id-or-General resolver the
//! kept owner-hint adapters and the location module share.
//!
//! **Folders (`4a38222e6`).** An item's `folder` is its path under `Wardrobe/`
//! and nothing else: [`list_folders_in_mount`] unions the store's folder rows
//! with the folders item paths imply, [`create_folder_in_mount`] /
//! [`rename_folder_in_mount`] / [`delete_folder_in_mount`] are the operator's
//! filing verbs, and `safe_folder` is the last line of defence for a `folder`
//! that reached a write un-resolved (a `.qtap` import's `../x`, a legacy
//! preset) — so every caller passes `folder` RAW.
//!
//! v4's per-mount write serialization (`runSerialized`) and the store's
//! `reservedBlobPaths` (R-B) are Node in-process concurrency guards, not on-disk
//! state — the single-writer model already serializes applies. NO-PORT.
//!
//! Every mount-level primitive takes the two connections (`main` for the
//! character rows the tombstone and the pool's group tier need; `mount` for
//! the document store) and answers a
//! [`WardrobeLocationError`](super::wardrobe_location::WardrobeLocationError);
//! the three pre-`9ecebf986` owner-hint adapters at the bottom keep their
//! `(main, links, docs)` shape and the old [`WardrobePublicError`] for the
//! import / restore writers that still address items by owner.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::Value;

use crate::clock;
use crate::vault_overlay::{
    detect_component_cycles, has_stable_wardrobe_item_id, is_wardrobe_item_document, WardrobeItem,
};
use crate::wardrobe_folders::{
    folder_name, folder_segments, is_safe_wardrobe_folder_path, is_same_folder, is_under_folder,
    join_folder, normalize_wardrobe_folder_path_or_root, parent_folder, rebase_folder,
    wardrobe_folder_store_path, WardrobeFolder, WardrobeFolderPathError,
    WARDROBE_RESERVED_FOLDER_SEGMENTS,
};

use super::archetype_wardrobe::read_general_wardrobe;
use super::database_store::{
    delete_database_folder, move_database_folder, DbStoreErrorCode, StoreError,
};
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::doc_mount_folders::DocMountFoldersRepository;
use super::instance_settings;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::vault_wardrobe_write::project_vault_wardrobe;
use super::wardrobe_location::WardrobeLocationError;
use super::{characters_read, DbError};

/// A failure on the pre-`9ecebf986` public write path (the kept owner-hint
/// adapters). `NoMount` / `Cycle` are v4's thrown `Error`s (the differential
/// compares their messages); `Db` wraps a storage error.
#[derive(Debug)]
pub enum WardrobePublicError {
    /// No vault mount resolved (missing character, unprovisioned store, or a
    /// `null` characterId with no General tier). v4 throws the "no Character Vault
    /// or Quilltap General mount is available" message.
    NoMount,
    /// The mutation would create a component cycle. Carries v4's exact message.
    Cycle(String),
    /// An underlying storage error.
    Db(DbError),
}

impl From<DbError> for WardrobePublicError {
    fn from(e: DbError) -> Self {
        WardrobePublicError::Db(e)
    }
}

/// v4's create/update/delete throw the SAME "no mount" message; expose it so the
/// differential can match the threw-string. (create says "create", update/delete
/// "update"/"delete", but all share this suffix — the differential keys on the
/// stable prefix.)
pub const NO_MOUNT_MESSAGE: &str = "no Character Vault or Quilltap General mount is available. \
Wardrobe items are stored exclusively in the document store.";

/// The four wardrobe tiers (v4 `WardrobeMountScope` / `WardrobeScopeSchema`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WardrobeScope {
    Character,
    Group,
    Project,
    General,
}

impl WardrobeScope {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            WardrobeScope::Character => "character",
            WardrobeScope::Group => "group",
            WardrobeScope::Project => "project",
            WardrobeScope::General => "general",
        }
    }
}

/// A wardrobe tier's folder on disk (v4 `WardrobeMount`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeMount {
    pub mount_point_id: String,
    pub scope: WardrobeScope,
    /// Owning character for the `character` scope; `None` for every shared
    /// tier.
    pub character_id: Option<String>,
}

impl WardrobeMount {
    /// The parse / log scope the reader and projector take — v4 `characterId
    /// ?? mountPointId`.
    pub fn scope_id(&self) -> &str {
        self.character_id
            .as_deref()
            .unwrap_or(self.mount_point_id.as_str())
    }
}

/// A partial wardrobe update (v4's `Partial<WardrobeItem>` patch, `{...cur,
/// ...patch}`) — by construction it cannot carry `id` / `createdAt` /
/// `updatedAt` ("Identity and timestamps are the writer's, never a patch's").
/// `None` = key absent (unchanged), `Some(None)` = set a nullable field to null.
#[derive(Debug, Default, Clone)]
pub struct WardrobePatch {
    pub title: Option<String>,
    pub types: Option<Vec<String>>,
    pub component_item_ids: Option<Vec<String>>,
    pub description: Option<Option<String>>,
    pub image_prompt: Option<Option<String>>,
    pub appropriateness: Option<Option<String>>,
    pub is_default: Option<bool>,
    pub replace: Option<bool>,
    pub archived_at: Option<Option<String>>,
    /// P4.D255 (v4 `updateWardrobeSchema.imageFileId`, `7c8572869`): choose
    /// among the item's own pictures (`Some(Some(id))`) or clear the pointer
    /// (`Some(None)`); the ownership check is the caller's
    /// (`services::wardrobe_item_images::primitives::assert_item_image_choice`).
    pub image_file_id: Option<Option<String>>,
    /// `4a38222e6`: the folder under `Wardrobe/` to file the item in. Applied
    /// through `safe_folder` — pass it RAW.
    pub folder: Option<String>,
    /// v4's `migratedFromClothingRecordId` is a plain item key a
    /// `Partial<WardrobeItem>` may carry (the restore pass does).
    pub migrated_from_clothing_record_id: Option<Option<String>>,
}

impl WardrobePatch {
    /// Overlay the present keys onto `item` (the `...patch` half of the merge).
    /// `folder` is NOT applied here — the writer does, through `safe_folder`.
    fn apply(&self, item: &mut WardrobeItem) {
        if let Some(v) = &self.title {
            item.title = v.clone();
        }
        if let Some(v) = &self.types {
            item.types = v.clone();
        }
        if let Some(v) = &self.component_item_ids {
            item.component_item_ids = v.clone();
        }
        if let Some(v) = &self.description {
            item.description = Some(v.clone());
        }
        if let Some(v) = &self.image_prompt {
            item.image_prompt = Some(v.clone());
        }
        if let Some(v) = &self.appropriateness {
            item.appropriateness = Some(v.clone());
        }
        if let Some(v) = self.is_default {
            item.is_default = v;
        }
        if let Some(v) = self.replace {
            item.replace = v;
        }
        if let Some(v) = &self.archived_at {
            item.archived_at = Some(v.clone());
        }
        if let Some(v) = &self.image_file_id {
            item.image_file_id = Some(v.clone());
        }
        if let Some(v) = &self.migrated_from_clothing_record_id {
            item.migrated_from_clothing_record_id = Some(v.clone());
        }
    }
}

/// One item's patch in an [`update_many_in_mount`] batch (v4 `WardrobeItemPatch`).
#[derive(Debug, Clone)]
pub struct WardrobeItemPatch {
    pub id: String,
    pub patch: WardrobePatch,
}

/// Options for [`update_many_in_mount`] (v4 `UpdateManyOptions`).
#[derive(Debug, Clone, Copy, Default)]
pub struct UpdateManyOptions {
    /// Refuse the whole batch — before anything is written — when any id isn't
    /// in the folder (`ItemNotInLocation`). Without it a missing id is skipped.
    pub require_all: bool,
}

/// What [`delete_folder_in_mount`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedFolder {
    pub moved_items: usize,
    pub moved_folders: usize,
    pub parent: String,
}

/// Resolve the folder for a character id, or Quilltap General for `None` (v4
/// `resolveWardrobeMount`). `Ok(None)` when no mount is available (no linked
/// vault, General not yet provisioned). [`DbError::CharacterArchived`] for an
/// archived character: a pruned vault is still live, so refusing here is what
/// keeps the tombstone.
pub fn resolve_wardrobe_mount(
    main: &Connection,
    character_id: Option<&str>,
) -> Result<Option<WardrobeMount>, DbError> {
    match character_id {
        None => {
            let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
                return Ok(None);
            };
            Ok(Some(WardrobeMount {
                mount_point_id,
                scope: WardrobeScope::General,
                character_id: None,
            }))
        }
        Some(cid) => {
            let Some(row) = characters_read::find_by_id_raw(main, cid)? else {
                return Ok(None);
            };
            // Archived characters keep a live vault (§4.2a prunes in place), so
            // the old null-mount skip no longer fires for them. Refuse rather
            // than answer `None` (v4 `wardrobe-writes.ts:92-107`, `d553f72a`).
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
            Ok(Some(WardrobeMount {
                mount_point_id: mount_point_id.to_string(),
                scope: WardrobeScope::Character,
                character_id: Some(cid.to_string()),
            }))
        }
    }
}

/// [`resolve_wardrobe_mount`] for the dressing-instructions route (v4
/// `b86bb1a5` calls the same resolver from `?action=instructions`): the
/// character's vault mount id, `None` when no vault is available, and
/// [`DbError::CharacterArchived`] for the tombstone — which the route turns into
/// its OWN 409 sentence, not this error's message.
pub fn resolve_character_wardrobe_mount_point(
    main: &Connection,
    character_id: &str,
) -> Result<Option<String>, DbError> {
    Ok(resolve_wardrobe_mount(main, Some(character_id))?.map(|m| m.mount_point_id))
}

/// v4 `mountWardrobeLocation(mountPointId, scope = 'project')` — a shared-tier
/// folder (project or group store) addressed by its mount.
pub fn mount_wardrobe_location(mount_point_id: &str, scope: WardrobeScope) -> WardrobeMount {
    WardrobeMount {
        mount_point_id: mount_point_id.to_string(),
        scope,
        character_id: None,
    }
}

/// Every item (archived included) currently in the folder, `characterId`
/// coerced to the folder's owner (v4 `readMountItems`). Component refs that
/// live in another tier stay as UUIDs (bug 187). Empty/missing folder → `[]`.
pub fn read_mount_items(
    mount: &Connection,
    loc: &WardrobeMount,
) -> Result<Vec<WardrobeItem>, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    let Some(vault) = read_character_vault_wardrobe(&docs, &loc.mount_point_id, loc.scope_id())?
    else {
        return Ok(Vec::new());
    };
    let items = vault
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .iter()
        .map(|it| {
            let mut item = WardrobeItem::from_read_value(it);
            item.character_id = Some(loc.character_id.clone());
            item
        })
        .collect())
}

/// The id→componentItemIds map a save-time cycle check walks (v4
/// `buildCyclePeers`): the folder's own items, then every other item the
/// folder's composites could reach. A character's composite can gather parts
/// from any tier the character wears from, so its peers are the character's
/// whole wearable pool (the pool built over `own_items: current` — the project
/// tier is NOT resolved: no chat); a shared composite's are General's items;
/// General's own folder is the whole set. A failed peer read WARNs `Wardrobe
/// cycle check could not load peer tiers; checking the folder alone`.
fn build_cycle_peers(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    current: &[WardrobeItem],
) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = current
        .iter()
        .map(|i| (i.id.clone(), i.component_item_ids.clone()))
        .collect();
    let mut add = |item: &Value| {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            return;
        };
        map.entry(id.to_string()).or_insert_with(|| {
            item.get("componentItemIds")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        });
    };
    if let Some(character_id) = &loc.character_id {
        let own: Vec<Value> = current
            .iter()
            .map(|i| serde_json::to_value(i).expect("serialize wardrobe item"))
            .collect();
        let pool = crate::wearable_pool::load_wearable_pool(
            main,
            mount,
            character_id,
            None,
            crate::wearable_pool::LoadWearablePoolOptions {
                own_items: Some(&own),
                ..Default::default()
            },
        );
        for item in pool.by_id_ordered() {
            add(item);
        }
    } else if loc.scope != WardrobeScope::General {
        let docs = DocMountDocumentsRepository::new(mount);
        match read_general_wardrobe(main, &docs, true) {
            Ok(items) => {
                for item in &items {
                    add(item);
                }
            }
            Err(error) => {
                tracing::warn!(
                    mountPointId = %loc.mount_point_id,
                    scope = loc.scope.as_str(),
                    context = "wardrobe",
                    error = %crate::db::fallback::error_text(&error),
                    "Wardrobe cycle check could not load peer tiers; checking the folder alone"
                );
            }
        }
    }
    map
}

/// v4's `assertNoCycles`: a non-composite passes; else `peers.set(item.id,
/// item)` (the map MUTATES, so later patches in a batch see earlier ones) then
/// reject any cycle with v4's exact message (` → ` between hops, `; ` between
/// distinct cycles).
fn assert_no_cycles(
    item: &WardrobeItem,
    peers: &mut HashMap<String, Vec<String>>,
) -> Result<(), WardrobeLocationError> {
    if item.component_item_ids.is_empty() {
        return Ok(());
    }
    peers.insert(item.id.clone(), item.component_item_ids.clone());
    let cycles = detect_component_cycles(&item.id, &item.component_item_ids, peers);
    if !cycles.is_empty() {
        let joined = cycles
            .iter()
            .map(|c| c.join(" \u{2192} "))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(WardrobeLocationError::Cycle(format!(
            "Wardrobe item {} would create a component cycle: {}",
            item.id, joined
        )));
    }
    Ok(())
}

/// The folder an item is written to (v4 `safeFolder`): a structurally safe
/// folder as given (normal form, or a hand-made folder the store already
/// holds), anything else (an imported bundle's `../x`) normalised, or the root
/// when it can't be. The operator-facing paths resolve folders first
/// (`resolve_wardrobe_folder`); this is the last line of defence for the ones
/// that don't.
fn safe_folder(folder: &str) -> String {
    if folder.is_empty() {
        return String::new();
    }
    if is_safe_wardrobe_folder_path(folder) {
        return folder.to_string();
    }
    let normalised = normalize_wardrobe_folder_path_or_root(Some(folder));
    tracing::debug!(
        folder = %folder,
        normalised = %normalised,
        context = "wardrobe",
        "Wardrobe write normalised an incoming folder"
    );
    normalised
}

fn project(mount: &Connection, loc: &WardrobeMount, items: &[WardrobeItem]) -> Result<(), DbError> {
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    project_vault_wardrobe(&links, &docs, &loc.mount_point_id, loc.scope_id(), items)
}

/// Create an item in the folder (v4 `createInMount`).
pub fn create_in_mount(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    item: &WardrobeItem,
) -> Result<WardrobeItem, WardrobeLocationError> {
    let current = read_mount_items(mount, loc)?;
    let mut peers = build_cycle_peers(main, mount, loc, &current);
    assert_no_cycles(item, &mut peers)?;
    // stored = { ...item, folder: safeFolder(item.folder), characterId: mount.characterId }
    let mut stored = item.clone();
    stored.folder = safe_folder(&item.folder);
    stored.character_id = Some(loc.character_id.clone());
    let mut next = current;
    next.push(stored.clone());
    project(mount, loc, &next)?;
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        wardrobeItemId = %stored.id,
        folder = %stored.folder,
        context = "wardrobe",
        "Wardrobe item created in folder"
    );
    Ok(stored)
}

/// Patch an item in the folder; `None` when the id isn't there (v4
/// `updateInMount` = `updateManyInMount` of one).
pub fn update_in_mount(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    id: &str,
    patch: &WardrobePatch,
) -> Result<Option<WardrobeItem>, WardrobeLocationError> {
    let mut updated = update_many_in_mount(
        main,
        mount,
        loc,
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

/// Patch several items in the folder with one read and **one** projection (v4
/// `updateManyInMount`). Returns the updated items in patch order; an id that
/// isn't in the folder is skipped (or, with `require_all`, refuses the batch —
/// BEFORE any write — naming the missing ids, deduped). `now` is minted once
/// per batch. A composite's cycle peers are built LAZILY ONCE against the
/// PRE-batch folder and mutate as merged items are checked. `movesFolder` is an
/// EXACT-case compare (a case-only folder change counts as a move — v4
/// `wardrobe-writes.ts:304`, ported as written, never "unified" with
/// `isSameFolder`'s case-blind skip in the mutations).
pub fn update_many_in_mount(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    patches: &[WardrobeItemPatch],
    opts: UpdateManyOptions,
) -> Result<Vec<WardrobeItem>, WardrobeLocationError> {
    if patches.is_empty() {
        return Ok(Vec::new());
    }
    let current = read_mount_items(mount, loc)?;
    update_many_unserialized(main, mount, loc, patches, &current, opts)
}

/// Delete an item from the folder; `false` when the id isn't there (no
/// projection) (v4 `deleteInMount`).
pub fn delete_in_mount(
    _main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    id: &str,
) -> Result<bool, WardrobeLocationError> {
    let current = read_mount_items(mount, loc)?;
    let next: Vec<WardrobeItem> = current.iter().filter(|i| i.id != id).cloned().collect();
    if next.len() == current.len() {
        return Ok(false);
    }
    project(mount, loc, &next)?;
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        wardrobeItemId = %id,
        context = "wardrobe",
        "Wardrobe item deleted from folder"
    );
    Ok(true)
}

// ============================================================================
// Folders — the operator's filing under `Wardrobe/`
// ============================================================================

/// The folder path under `Wardrobe/` for a store folder path, or `None` when
/// the path is `Wardrobe` itself, outside it, or under the reserved `images/`
/// (v4 `folderUnderWardrobe`).
pub fn folder_under_wardrobe(store_path: &str) -> Option<String> {
    let prefix = "Wardrobe/";
    if !store_path
        .to_lowercase()
        .starts_with(&prefix.to_lowercase())
    {
        return None;
    }
    let folder = store_path.get(prefix.len()..).unwrap_or("");
    if folder.is_empty() {
        return None;
    }
    let first = folder_segments(folder)[0].to_lowercase();
    if WARDROBE_RESERVED_FOLDER_SEGMENTS.contains(&first.as_str()) {
        return None;
    }
    Some(folder.to_string())
}

/// The stored casing of a folder that exists, or `None` (v4
/// `findStoredFolder` — the store's path lookup is case-insensitive, a
/// fallback read).
pub fn find_stored_folder(
    mount: &Connection,
    mount_point_id: &str,
    folder: &str,
) -> Option<String> {
    DocMountFoldersRepository::new(mount)
        .find_by_mount_point_and_path_or_none(mount_point_id, &wardrobe_folder_store_path(folder))
        .and_then(|row| folder_under_wardrobe(&row.path))
}

fn is_live(item: &WardrobeItem) -> bool {
    !matches!(&item.archived_at, Some(Some(a)) if !a.is_empty())
}

/// Every folder under the tier's `Wardrobe/` (v4 `listFoldersInMount`): the
/// union of the store's folder rows (empty folders persist as rows) and the
/// folders implied by item paths, minus `images/`; every ancestor is a folder
/// too, the first spelling per lower-cased key winning. `itemCount` counts
/// non-archived items directly inside. Sorted by path, case-insensitively
/// (`localeCompare(…, { sensitivity: 'base' })`). DEBUG `Wardrobe folders
/// listed`.
pub fn list_folders_in_mount(
    mount: &Connection,
    loc: &WardrobeMount,
) -> Result<Vec<WardrobeFolder>, DbError> {
    let rows =
        DocMountFoldersRepository::new(mount).find_by_mount_point_id_or_empty(&loc.mount_point_id);
    let items = read_mount_items(mount, loc)?;

    let mut by_key: HashMap<String, String> = HashMap::new();
    let mut add = |folder: &str| {
        let segments = folder_segments(folder);
        for depth in 1..=segments.len() {
            let path = segments[..depth].join("/");
            by_key.entry(path.to_lowercase()).or_insert(path);
        }
    };
    for row in &rows {
        if let Some(folder) = folder_under_wardrobe(&row.path) {
            add(&folder);
        }
    }
    for item in &items {
        if !item.folder.is_empty() {
            add(&item.folder);
        }
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for item in &items {
        if !is_live(item) || item.folder.is_empty() {
            continue;
        }
        *counts.entry(item.folder.to_lowercase()).or_insert(0) += 1;
    }

    let mut folders: Vec<WardrobeFolder> = by_key
        .iter()
        .map(|(key, path)| WardrobeFolder {
            path: path.clone(),
            name: folder_name(path).to_string(),
            item_count: counts.get(key).copied().unwrap_or(0),
        })
        .collect();
    folders.sort_by(|a, b| crate::collation::locale_compare_base(&a.path, &b.path));
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        folderRowCount = rows.len(),
        folderCount = folders.len(),
        context = "wardrobe",
        "Wardrobe folders listed"
    );
    Ok(folders)
}

/// The paths of every folder under the tier's `Wardrobe/`, from the store's
/// folder rows alone (every item's folder has one — writes create them), minus
/// `images/` (v4 `listFolderPathsInMount`). One query, no item parse: what
/// `resolve_wardrobe_folder` resolves an operator's request against — so an
/// item-implied folder with no row is invisible to resolution. A fallback
/// read: never fails.
pub fn list_folder_paths_in_mount(mount: &Connection, loc: &WardrobeMount) -> Vec<String> {
    DocMountFoldersRepository::new(mount)
        .find_by_mount_point_id_or_empty(&loc.mount_point_id)
        .iter()
        .filter_map(|row| folder_under_wardrobe(&row.path))
        .collect()
}

/// Create a folder (and its missing ancestors) (v4 `createFolderInMount`).
/// Idempotent and case-insensitive: creating `formal` beside `Formal` answers
/// `Formal`. `folder` must already be normalised; `''` is refused (`The
/// wardrobe root already exists`). DEBUG `Wardrobe folder ensured`.
pub fn create_folder_in_mount(
    mount: &Connection,
    loc: &WardrobeMount,
    folder: &str,
) -> Result<WardrobeFolder, WardrobeLocationError> {
    if folder.is_empty() {
        return Err(WardrobeFolderPathError("The wardrobe root already exists".to_string()).into());
    }
    let links = DocMountFileLinksRepository::new(mount);
    links.ensure_folder_path(&loc.mount_point_id, &wardrobe_folder_store_path(folder))?;
    let stored = find_stored_folder(mount, &loc.mount_point_id, folder)
        .unwrap_or_else(|| folder.to_string());
    let items = read_mount_items(mount, loc)?;
    let item_count = items
        .iter()
        .filter(|i| is_live(i) && is_same_folder(Some(&i.folder), Some(&stored)))
        .count();
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        requested = %folder,
        folder = %stored,
        itemCount = item_count,
        context = "wardrobe",
        "Wardrobe folder ensured"
    );
    Ok(WardrobeFolder {
        name: folder_name(&stored).to_string(),
        path: stored,
        item_count,
    })
}

/// Map a store-level failure of a folder move / delete onto the location's
/// error vocabulary (v4's `DatabaseStoreError` code switches). A code the
/// caller does not map is v4's `throw error` — the store's own sentence as-is.
fn store_error(
    e: StoreError,
    conflict: Option<&dyn Fn() -> WardrobeLocationError>,
    not_found: Option<&dyn Fn() -> WardrobeLocationError>,
    not_empty: Option<&dyn Fn() -> WardrobeLocationError>,
) -> WardrobeLocationError {
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
                None => WardrobeLocationError::Db(DbError::Internal(s.message)),
            }
        }
        StoreError::Db(d) => d.into(),
    }
}

/// Rename a folder in place (`new_name` is one normalised segment) (v4
/// `renameFolderInMount`). Its items, subfolders and any other files move with
/// it — the path is the truth, so every item's `folder` follows. A sibling
/// already holding the name (in any casing) is a `FolderConflict`; a case-only
/// rename is allowed. The store moves the files without rewriting them, and an
/// item file with no frontmatter `id` takes its id from its path — moving it
/// would mint a new one and orphan its outfit slots, ledger, outfits and
/// pictures — so every such file is STAMPED with the id it has now, first
/// (one projection of the whole tier; DEBUG `Stamping ids into wardrobe files
/// before a folder rename`). DEBUG `Wardrobe folder renamed`.
pub fn rename_folder_in_mount(
    mount: &Connection,
    loc: &WardrobeMount,
    folder: &str,
    new_name: &str,
) -> Result<WardrobeFolder, WardrobeLocationError> {
    if folder.is_empty() {
        return Err(
            WardrobeFolderPathError("The wardrobe root cannot be renamed".to_string()).into(),
        );
    }
    let Some(from) = find_stored_folder(mount, &loc.mount_point_id, folder) else {
        return Err(WardrobeLocationError::FolderNotFound {
            folder: folder.to_string(),
        });
    };
    let to = join_folder(&[parent_folder(&from), new_name]);
    if to != from {
        let docs = DocMountDocumentsRepository::new(mount);
        let docs_in = docs.find_many_by_mount_points_in_folder_opts_or_empty(
            std::slice::from_ref(&loc.mount_point_id),
            &wardrobe_folder_store_path(&from),
            ".md",
            true,
        )?;
        let unstamped = docs_in
            .iter()
            .filter(|d| {
                is_wardrobe_item_document(&d.relative_path, Some(&d.content))
                    && !has_stable_wardrobe_item_id(Some(&d.content))
            })
            .count();
        if unstamped > 0 {
            tracing::debug!(
                mountPointId = %loc.mount_point_id,
                folder = %from,
                count = unstamped,
                context = "wardrobe",
                "Stamping ids into wardrobe files before a folder rename"
            );
            let items = read_mount_items(mount, loc)?;
            project(mount, loc, &items)?;
        }
        move_database_folder(
            mount,
            &loc.mount_point_id,
            &wardrobe_folder_store_path(&from),
            &wardrobe_folder_store_path(&to),
        )
        .map_err(|e| {
            store_error(
                e,
                Some(&|| WardrobeLocationError::folder_conflict(&to)),
                Some(&|| WardrobeLocationError::FolderNotFound {
                    folder: folder.to_string(),
                }),
                None,
            )
        })?;
    }
    let items = read_mount_items(mount, loc)?;
    let item_count = items
        .iter()
        .filter(|i| is_live(i) && is_same_folder(Some(&i.folder), Some(&to)))
        .count();
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        from = %from,
        to = %to,
        itemCount = item_count,
        context = "wardrobe",
        "Wardrobe folder renamed"
    );
    Ok(WardrobeFolder {
        name: folder_name(&to).to_string(),
        path: to,
        item_count,
    })
}

/// Delete a folder, sending everything in it up one level (v4
/// `deleteFolderInMount`): its items (archived included) to the parent, its
/// subfolders to sit beside it — merging into a same-named folder there,
/// case-insensitively. Nothing is deleted but folder rows. Everything is
/// checked BEFORE anything moves, so a refusal leaves the wardrobe exactly as
/// it was: (1) files the wardrobe doesn't own — a picture, a `.txt`, a note or
/// a nested `instructions.md` — refuse with `Folder "<f>" holds N file(s) that
/// are not wardrobe items; move them in the Scriptorium first`; (2) a
/// subfolder sharing the folder's own name (`Formal/Formal`) would be sent up
/// into the very place being removed — `Folder "<f>" holds a subfolder of the
/// same name; rename one of them first`. Items go in ONE batch (one
/// projection), each re-rooted via `rebase_folder`; subfolders are re-created
/// (empty ones included) under the parent, then the old rows dropped deepest
/// first; a late `NOT_EMPTY` is `Folder "<f>" is not empty yet; its items were
/// moved up`. DEBUG `Wardrobe folder deleted; contents sent up`.
pub fn delete_folder_in_mount(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    folder: &str,
) -> Result<DeletedFolder, WardrobeLocationError> {
    if folder.is_empty() {
        return Err(
            WardrobeFolderPathError("The wardrobe root cannot be deleted".to_string()).into(),
        );
    }
    let Some(from) = find_stored_folder(mount, &loc.mount_point_id, folder) else {
        return Err(WardrobeLocationError::FolderNotFound {
            folder: folder.to_string(),
        });
    };
    let parent = parent_folder(&from).to_string();
    let store_from = wardrobe_folder_store_path(&from);
    let store_prefix = format!("{store_from}/").to_lowercase();

    let links_repo = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    let folders = DocMountFoldersRepository::new(mount);

    // 1. Strays.
    let links: Vec<_> = links_repo
        .find_by_mount_point_id_or_empty(&loc.mount_point_id)
        .into_iter()
        .filter(|l| l.relative_path.to_lowercase().starts_with(&store_prefix))
        .collect();
    let docs_in = docs.find_many_by_mount_points_in_folder_opts_or_empty(
        std::slice::from_ref(&loc.mount_point_id),
        &store_from,
        ".md",
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
                Some(content) => !is_wardrobe_item_document(&l.relative_path, Some(content)),
            },
        )
        .count();
    if strays > 0 {
        return Err(WardrobeLocationError::FolderConflict {
            folder: from.clone(),
            message: format!(
                "Folder \"{from}\" holds {strays} file(s) that are not wardrobe items; move them in the Scriptorium first"
            ),
        });
    }

    // 2. A subfolder sharing the folder's own name.
    let mut rows: Vec<_> = folders
        .find_by_mount_point_id_or_empty(&loc.mount_point_id)
        .into_iter()
        .filter(|r| r.path.to_lowercase().starts_with(&store_prefix))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.path.len()));
    let items = read_mount_items(mount, loc)?;
    let moving: Vec<&WardrobeItem> = items
        .iter()
        .filter(|item| is_under_folder(&item.folder, &from))
        .collect();
    let lands_inside = moving
        .iter()
        .map(|item| item.folder.clone())
        .chain(
            rows.iter()
                .map(|row| folder_under_wardrobe(&row.path).unwrap_or_default()),
        )
        .any(|sub| !sub.is_empty() && is_under_folder(&rebase_folder(&sub, &from, &parent), &from));
    if lands_inside {
        return Err(WardrobeLocationError::FolderConflict {
            folder: from.clone(),
            message: format!(
                "Folder \"{from}\" holds a subfolder of the same name; rename one of them first"
            ),
        });
    }

    // Items: one batch, one projection.
    let moved_items = moving.len();
    if moved_items > 0 {
        let patches: Vec<WardrobeItemPatch> = moving
            .iter()
            .map(|item| WardrobeItemPatch {
                id: item.id.clone(),
                patch: WardrobePatch {
                    folder: Some(rebase_folder(&item.folder, &from, &parent)),
                    ..Default::default()
                },
            })
            .collect();
        // v4 `updateManyUnserialized`: on the mount's own chain, over the items
        // already read — and, as every batch does, re-checking each moved
        // composite against its peers (a re-rooting patch never changes
        // components, but v4 checks whatever it merges).
        update_many_unserialized(
            main,
            mount,
            loc,
            &patches,
            &items,
            UpdateManyOptions::default(),
        )?;
    }

    // Subfolders: re-create each (empty ones included) under the parent, then
    // drop the old rows deepest first.
    for row in &rows {
        let Some(sub) = folder_under_wardrobe(&row.path) else {
            continue;
        };
        links_repo.ensure_folder_path(
            &loc.mount_point_id,
            &wardrobe_folder_store_path(&rebase_folder(&sub, &from, &parent)),
        )?;
    }
    let not_empty = || WardrobeLocationError::FolderConflict {
        folder: from.clone(),
        message: format!("Folder \"{from}\" is not empty yet; its items were moved up"),
    };
    for row in &rows {
        delete_database_folder(mount, &loc.mount_point_id, &row.path)
            .map_err(|e| store_error(e, None, None, Some(&not_empty)))?;
    }
    delete_database_folder(mount, &loc.mount_point_id, &store_from)
        .map_err(|e| store_error(e, None, None, Some(&not_empty)))?;

    let child_depth = folder_segments(&from).len() + 1;
    let moved_folders = rows
        .iter()
        .filter(|row| {
            folder_segments(&folder_under_wardrobe(&row.path).unwrap_or_default()).len()
                == child_depth
        })
        .count();
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        folder = %from,
        parent = %parent,
        movedItems = moved_items,
        movedFolders = moved_folders,
        context = "wardrobe",
        "Wardrobe folder deleted; contents sent up"
    );
    Ok(DeletedFolder {
        moved_items,
        moved_folders,
        parent,
    })
}

/// The body of [`update_many_in_mount`] over an ALREADY-READ folder — v4
/// `updateManyUnserialized`, which delete-folder calls on the mount's own chain.
/// The `require_all` refusal comes first, BEFORE any write, naming the missing
/// ids deduped; `now` is minted once per batch; a composite's cycle peers are
/// built LAZILY ONCE against the PRE-batch folder and mutate as merged items
/// are checked; `movesFolder` is an EXACT-case compare (a case-only folder
/// change counts as a move — v4 `wardrobe-writes.ts:304`, ported as written,
/// never "unified" with `isSameFolder`'s case-blind skip in the mutations).
fn update_many_unserialized(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    patches: &[WardrobeItemPatch],
    current: &[WardrobeItem],
    opts: UpdateManyOptions,
) -> Result<Vec<WardrobeItem>, WardrobeLocationError> {
    let index_by_id: HashMap<&str, usize> = current
        .iter()
        .enumerate()
        .map(|(i, item)| (item.id.as_str(), i))
        .collect();
    if opts.require_all {
        let mut missing: Vec<String> = Vec::new();
        for p in patches {
            if !index_by_id.contains_key(p.id.as_str()) && !missing.contains(&p.id) {
                missing.push(p.id.clone());
            }
        }
        if !missing.is_empty() {
            return Err(WardrobeLocationError::ItemNotInLocation { item_ids: missing });
        }
    }
    let mut next = current.to_vec();
    let now = clock::now_iso();
    let mut updated_ids: Vec<String> = Vec::new();
    let mut peers: Option<HashMap<String, Vec<String>>> = None;
    let mut moves_folder = false;

    for WardrobeItemPatch { id, patch } in patches {
        let Some(&idx) = index_by_id.get(id.as_str()) else {
            continue;
        };
        let before = &next[idx];
        // { ...before, ...patch, ...(folder && { folder: safeFolder }), id,
        //   characterId: mount.characterId, createdAt: before.createdAt, updatedAt: now }
        let mut merged = before.clone();
        patch.apply(&mut merged);
        if let Some(folder) = &patch.folder {
            merged.folder = safe_folder(folder);
        }
        merged.id = before.id.clone();
        merged.character_id = Some(loc.character_id.clone());
        merged.created_at = before.created_at.clone();
        merged.updated_at = now.clone();
        if !merged.component_item_ids.is_empty() {
            let peers = peers.get_or_insert_with(|| build_cycle_peers(main, mount, loc, current));
            assert_no_cycles(&merged, peers)?;
        }
        if merged.folder != before.folder {
            moves_folder = true;
        }
        next[idx] = merged;
        updated_ids.push(id.clone());
    }
    if updated_ids.is_empty() {
        return Ok(Vec::new());
    }

    project(mount, loc, &next)?;
    tracing::debug!(
        mountPointId = %loc.mount_point_id,
        scope = loc.scope.as_str(),
        wardrobeItemIds = %serde_json::to_string(&updated_ids).unwrap_or_default(),
        count = updated_ids.len(),
        movesFolder = moves_folder,
        context = "wardrobe",
        "Wardrobe items updated in folder"
    );

    Ok(updated_ids
        .iter()
        .map(|id| next[index_by_id[id.as_str()]].clone())
        .collect())
}

// ============================================================================
// The owner-hint adapters v4 KEPT (`WardrobeRepository.create/update/delete`)
// ============================================================================

/// v4 `WardrobeRepository.create` → `createInMount(mountFor(characterId))`.
/// `item` already carries its id/createdAt/updatedAt (the public repo
/// materializes them before this point); its `characterId` selects the tier —
/// `Some` → that character's vault, `None` → Quilltap General. Returns the
/// stored item; `NoMount` when no vault resolves (v4 throws). Kept for the
/// import and restore writers, which address items by owner; everything else
/// creates through a location.
pub fn create_vault_wardrobe_item(
    main: &Connection,
    _links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    item: &WardrobeItem,
) -> Result<WardrobeItem, WardrobePublicError> {
    let character_id = item_character_id(item);
    let Some(loc) = resolve_wardrobe_mount(main, character_id.as_deref())? else {
        return Err(WardrobePublicError::NoMount);
    };
    Ok(create_in_mount(main, docs.conn(), &loc, item)?)
}

/// v4 `WardrobeRepository.update(id, patch, ownerCharacterId)` → `updateInMount`.
/// `character_id_hint` locates the mount (`None` → Quilltap General). `Ok(None)`
/// when the id isn't in the folder; `NoMount` when no vault resolves.
pub fn update_vault_wardrobe_item(
    main: &Connection,
    _links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    id: &str,
    patch: &WardrobePatch,
    character_id_hint: Option<&str>,
) -> Result<Option<WardrobeItem>, WardrobePublicError> {
    let Some(loc) = resolve_wardrobe_mount(main, character_id_hint)? else {
        return Err(WardrobePublicError::NoMount);
    };
    Ok(update_in_mount(main, docs.conn(), &loc, id, patch)?)
}

/// v4 `WardrobeRepository.delete(id, ownerCharacterId)` → `deleteInMount`.
pub fn delete_vault_wardrobe_item(
    main: &Connection,
    _links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    id: &str,
    character_id_hint: Option<&str>,
) -> Result<bool, WardrobePublicError> {
    let Some(loc) = resolve_wardrobe_mount(main, character_id_hint)? else {
        return Err(WardrobePublicError::NoMount);
    };
    Ok(delete_in_mount(main, docs.conn(), &loc, id)?)
}

// LANE-REMOVE(P4.D281|P4.D284): v4 deleted createProjectWardrobeItem in 9ecebf986 (→ createMountWardrobeItem / a location)
/// The pre-`9ecebf986` project-store create over
/// `mount_wardrobe_location(mp, Project)` (v4's `createMountWardrobeItem`
/// shape).
pub fn create_project_wardrobe_item(
    main: &Connection,
    _links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    item: &WardrobeItem,
) -> Result<WardrobeItem, WardrobePublicError> {
    Ok(create_in_mount(
        main,
        docs.conn(),
        &mount_wardrobe_location(mount_point_id, WardrobeScope::Project),
        item,
    )?)
}

// LANE-REMOVE(P4.D281|P4.D284|P4.D283): v4 deleted updateProjectWardrobeItem in 9ecebf986 (→ updateMountWardrobeItem / a location)
/// The pre-`9ecebf986` project-store update (v4's `updateMountWardrobeItem`
/// shape — "the restore pass uses this for every tier").
pub fn update_project_wardrobe_item(
    main: &Connection,
    _links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    id: &str,
    patch: &WardrobePatch,
) -> Result<Option<WardrobeItem>, WardrobePublicError> {
    Ok(update_in_mount(
        main,
        docs.conn(),
        &mount_wardrobe_location(mount_point_id, WardrobeScope::Project),
        id,
        patch,
    )?)
}

// LANE-REMOVE(P4.D281): v4 deleted deleteProjectWardrobeItem in 9ecebf986 (→ deleteMountWardrobeItem / a location)
/// The pre-`9ecebf986` project-store delete.
pub fn delete_project_wardrobe_item(
    main: &Connection,
    _links: &DocMountFileLinksRepository,
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    id: &str,
) -> Result<bool, WardrobePublicError> {
    Ok(delete_in_mount(
        main,
        docs.conn(),
        &mount_wardrobe_location(mount_point_id, WardrobeScope::Project),
        id,
    )?)
}

/// The owning character id of an item, or `None` for a shared archetype (`null`).
fn item_character_id(item: &WardrobeItem) -> Option<String> {
    match &item.character_id {
        Some(Some(s)) => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, comps: &[&str]) -> WardrobeItem {
        WardrobeItem {
            id: id.to_string(),
            character_id: Some(Some("char".to_string())),
            title: format!("Item {id}"),
            description: None,
            image_prompt: None,
            types: vec!["top".to_string()],
            component_item_ids: comps.iter().map(|s| s.to_string()).collect(),
            appropriateness: None,
            is_default: false,
            replace: false,
            migrated_from_clothing_record_id: None,
            image_file_id: None,
            archived_at: None,
            folder: String::new(),
            created_at: "2026-02-01T00:00:00.000Z".to_string(),
            updated_at: "2026-02-01T00:00:00.000Z".to_string(),
        }
    }

    fn peers(current: &[WardrobeItem]) -> HashMap<String, Vec<String>> {
        current
            .iter()
            .map(|i| (i.id.clone(), i.component_item_ids.clone()))
            .collect()
    }

    #[test]
    fn patch_apply_overlays_present_keys_only_and_leaves_folder_to_the_writer() {
        let mut it = item("a", &[]);
        let patch = WardrobePatch {
            title: Some("New Title".into()),
            is_default: Some(true),
            description: Some(Some("desc".into())),
            folder: Some("Formal".into()),
            ..Default::default()
        };
        patch.apply(&mut it);
        assert_eq!(it.title, "New Title");
        assert!(it.is_default);
        assert_eq!(it.description, Some(Some("desc".to_string())));
        assert_eq!(it.types, vec!["top".to_string()]);
        assert!(!it.replace);
        assert_eq!(it.folder, "", "folder goes through safe_folder, not apply");
    }

    #[test]
    fn cycles_are_rejected_with_v4_message_and_peers_mutate() {
        assert!(assert_no_cycles(&item("a", &[]), &mut peers(&[])).is_ok());
        let current = vec![item("b", &[])];
        assert!(assert_no_cycles(&item("a", &["b"]), &mut peers(&current)).is_ok());
        let err = assert_no_cycles(&item("a", &["a"]), &mut peers(&[])).unwrap_err();
        assert!(err
            .message()
            .starts_with("Wardrobe item a would create a component cycle: "));
        assert!(err.message().contains("a \u{2192} a"));
        let current = vec![item("b", &["a"])];
        assert!(matches!(
            assert_no_cycles(&item("a", &["b"]), &mut peers(&current)),
            Err(WardrobeLocationError::Cycle(_))
        ));
    }

    #[test]
    fn safe_folder_keeps_safe_spellings_and_tames_the_rest() {
        assert_eq!(safe_folder(""), "");
        assert_eq!(safe_folder("Hats: Summer"), "Hats: Summer");
        assert_eq!(safe_folder("Formal/images"), "Formal/images");
        assert_eq!(safe_folder("../x"), "");
        assert_eq!(safe_folder("images/x"), "");
        assert_eq!(safe_folder("a//b"), "a/b");
    }

    #[test]
    fn folder_under_wardrobe_rules() {
        assert_eq!(
            folder_under_wardrobe("Wardrobe/A/B"),
            Some("A/B".to_string())
        );
        assert_eq!(folder_under_wardrobe("wardrobe/A"), Some("A".to_string()));
        assert_eq!(folder_under_wardrobe("Wardrobe"), None);
        assert_eq!(folder_under_wardrobe("Wardrobe/"), None);
        assert_eq!(folder_under_wardrobe("Wardrobe/images/x"), None);
        assert_eq!(
            folder_under_wardrobe("Wardrobe/A/images"),
            Some("A/images".to_string())
        );
        assert_eq!(folder_under_wardrobe("Notes/A"), None);
    }
}
