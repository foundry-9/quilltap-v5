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
    is_safe_wardrobe_folder_path, normalize_wardrobe_folder_path_or_root, WardrobeFolder,
    WardrobeFolderPathError,
};

use super::archetype_wardrobe::read_general_wardrobe;
use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::vault_wardrobe_write::project_vault_wardrobe;
use super::wardrobe_location::WardrobeLocationError;
use super::DbError;

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

/// The four wardrobe tiers (v4 `WardrobeMountScope` — since `4fbdad186` a
/// type alias of the shared `VaultFolderMountScope`; P4.D324's lift).
pub type WardrobeScope = super::item_folder_ops::VaultFolderMountScope;

/// A wardrobe tier's folder on disk (v4 `WardrobeMount = VaultFolderMount`).
pub type WardrobeMount = super::item_folder_ops::VaultFolderMount;

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
pub use super::item_folder_ops::DeletedFolder;

/// Resolve the folder for a character id, or Quilltap General for `None` (v4
/// `resolveWardrobeMount`). `Ok(None)` when no mount is available (no linked
/// vault, General not yet provisioned). [`DbError::CharacterArchived`] for an
/// archived character: a pruned vault is still live, so refusing here is what
/// keeps the tombstone.
pub fn resolve_wardrobe_mount(
    main: &Connection,
    character_id: Option<&str>,
) -> Result<Option<WardrobeMount>, DbError> {
    super::item_folder_ops::resolve_vault_folder_mount(main, character_id)
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
    crate::inventory::item_folders::folder_under_item_root(
        &crate::wardrobe_folders::WARDROBE_FOLDER_RULES,
        store_path,
    )
}

/// The stored casing of a folder that exists, or `None` (v4
/// `findStoredFolder` — the store's path lookup is case-insensitive, a
/// fallback read).
pub fn find_stored_folder(
    mount: &Connection,
    mount_point_id: &str,
    folder: &str,
) -> Option<String> {
    super::item_folder_ops::find_stored_folder(
        &crate::wardrobe_folders::WARDROBE_FOLDER_RULES,
        mount,
        mount_point_id,
        folder,
    )
}

/// The wardrobe's folder tree, as the shared folder operations see it (v4
/// `WARDROBE_TREE`, `wardrobe-writes.ts:296-316`): `.md` items, context
/// `wardrobe`, the wardrobe's predicates, `readMountItems`,
/// `projectVaultWardrobe`, and a refile through `updateManyUnserialized` over
/// the folder the delete already read (it re-checks each moved composite).
pub struct WardrobeTree;

impl super::item_folder_ops::ItemFolderTree for WardrobeTree {
    type Item = WardrobeItem;
    type Error = WardrobeLocationError;

    fn rules(&self) -> &'static crate::inventory::item_folders::ItemFolderRules {
        &crate::wardrobe_folders::WARDROBE_FOLDER_RULES
    }
    fn ext(&self) -> &'static str {
        ".md"
    }
    fn context(&self) -> &'static str {
        "wardrobe"
    }
    fn is_item_document(&self, relative_path: &str, content: Option<&str>) -> bool {
        is_wardrobe_item_document(relative_path, content)
    }
    fn has_stable_id(&self, content: Option<&str>) -> bool {
        has_stable_wardrobe_item_id(content)
    }
    fn filed(&self, item: &WardrobeItem) -> super::item_folder_ops::FiledItem {
        super::item_folder_ops::FiledItem {
            id: item.id.clone(),
            folder: item.folder.clone(),
            archived: matches!(&item.archived_at, Some(Some(a)) if !a.is_empty()),
        }
    }
    fn read(&self, mount: &Connection, m: &WardrobeMount) -> Result<Vec<WardrobeItem>, DbError> {
        read_mount_items(mount, m)
    }
    fn reproject(
        &self,
        mount: &Connection,
        m: &WardrobeMount,
        items: &[WardrobeItem],
    ) -> Result<(), DbError> {
        project(mount, m, items)
    }
    fn refile(
        &self,
        main: &Connection,
        mount: &Connection,
        m: &WardrobeMount,
        current: &[WardrobeItem],
        patches: &[(String, String)],
    ) -> Result<(), WardrobeLocationError> {
        let patches: Vec<WardrobeItemPatch> = patches
            .iter()
            .map(|(id, folder)| WardrobeItemPatch {
                id: id.clone(),
                patch: WardrobePatch {
                    folder: Some(folder.clone()),
                    ..Default::default()
                },
            })
            .collect();
        update_many_unserialized(
            main,
            mount,
            m,
            &patches,
            current,
            UpdateManyOptions::default(),
        )
        .map(|_| ())
    }
    fn path_error(&self, message: String) -> WardrobeLocationError {
        WardrobeFolderPathError(message).into()
    }
    fn not_found(&self, folder: &str) -> WardrobeLocationError {
        WardrobeLocationError::FolderNotFound {
            folder: folder.to_string(),
        }
    }
    fn conflict(&self, folder: &str, message: Option<String>) -> WardrobeLocationError {
        match message {
            Some(message) => WardrobeLocationError::FolderConflict {
                folder: folder.to_string(),
                message,
            },
            None => WardrobeLocationError::folder_conflict(folder),
        }
    }
}

/// Every folder under the tier's `Wardrobe/` (v4 `listFoldersInMount` — a
/// delegate over the shared `listFoldersIn`). DEBUG `Wardrobe folders listed`.
pub fn list_folders_in_mount(
    mount: &Connection,
    loc: &WardrobeMount,
) -> Result<Vec<WardrobeFolder>, DbError> {
    super::item_folder_ops::list_folders_in(&WardrobeTree, mount, loc)
}

/// The paths of every folder under the tier's `Wardrobe/`, from the store's
/// folder rows alone, minus `images/` (v4 `listFolderPathsInMount`). A
/// fallback read: never fails.
pub fn list_folder_paths_in_mount(mount: &Connection, loc: &WardrobeMount) -> Vec<String> {
    super::item_folder_ops::list_folder_paths_in(&WardrobeTree, mount, loc)
}

/// Create a folder (and its missing ancestors) (v4 `createFolderInMount`).
/// DEBUG `Wardrobe folder ensured`.
pub fn create_folder_in_mount(
    mount: &Connection,
    loc: &WardrobeMount,
    folder: &str,
) -> Result<WardrobeFolder, WardrobeLocationError> {
    super::item_folder_ops::create_folder_in(&WardrobeTree, mount, loc, folder)
}

/// Rename a folder in place (v4 `renameFolderInMount`). DEBUG `Wardrobe
/// folder renamed` (and `Stamping ids into wardrobe files before a folder
/// rename` when an id-less file moves).
pub fn rename_folder_in_mount(
    mount: &Connection,
    loc: &WardrobeMount,
    folder: &str,
    new_name: &str,
) -> Result<WardrobeFolder, WardrobeLocationError> {
    // `Wardrobe/` is never a title-lint path (v4 does not wrap the wardrobe).
    super::item_folder_ops::rename_folder_in(
        &WardrobeTree,
        mount,
        loc,
        folder,
        new_name,
        crate::inventory::lint_scope::TitleLintScope::NeverClaimed,
    )
}

/// Delete a folder, sending everything in it up one level (v4
/// `deleteFolderInMount`). A folder holding files that are not wardrobe items
/// (a picture, a nested `instructions.md`, a `.txt`) is refused before
/// anything moves. DEBUG `Wardrobe folder deleted; contents sent up`.
pub fn delete_folder_in_mount(
    main: &Connection,
    mount: &Connection,
    loc: &WardrobeMount,
    folder: &str,
) -> Result<DeletedFolder, WardrobeLocationError> {
    super::item_folder_ops::delete_folder_in(&WardrobeTree, main, mount, loc, folder)
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

    // v4 `listFoldersInMount`'s insertion-ordered `Map` + stable sort: rows
    // first, then items, first spelling per lower-cased key; base-equal paths
    // with distinct keys keep that order.
    #[test]
    fn folder_listing_keeps_v4s_insertion_order_for_base_equal_paths() {
        let variants = ["cafe", "Café", "cafè", "cafê", "cafë", "cafē"];
        let mut items: Vec<WardrobeItem> = Vec::new();
        for (i, v) in variants.iter().enumerate().skip(3) {
            let mut it = item(&format!("i{i}"), &[]);
            it.folder = format!("{v}/Inner");
            items.push(it);
        }
        // An archived item counts toward no folder; `CAFE` is the same key as
        // the row's `cafe` and loses to the first spelling.
        let mut archived = item("gone", &[]);
        archived.folder = "cafe".to_string();
        archived.archived_at = Some(Some("2026-03-01T00:00:00.000Z".to_string()));
        items.push(archived);
        let mut upper = item("upper", &[]);
        upper.folder = "CAFE".to_string();
        items.push(upper);
        let rows = variants[..3].iter().map(|v| v.to_string());

        use crate::db::item_folder_ops::ItemFolderTree;
        let filed: Vec<_> = items.iter().map(|i| WardrobeTree.filed(i)).collect();
        let got = crate::db::item_folder_ops::assemble_folders(rows, &filed);
        let paths: Vec<&str> = got.iter().map(|f| f.path.as_str()).collect();
        // Every top-level variant is base-equal, so the stable sort leaves
        // them in insertion order (rows, then items) — and each `/Inner`
        // sorts after the whole tied run, in the same order.
        assert_eq!(
            paths,
            vec![
                "cafe",
                "Café",
                "cafè",
                "cafê",
                "cafë",
                "cafē",
                "cafê/Inner",
                "cafë/Inner",
                "cafē/Inner",
            ]
        );
        let cafe = got.iter().find(|f| f.path == "cafe").unwrap();
        assert_eq!(
            cafe.item_count, 1,
            "CAFE counts under cafe; the archived one does not"
        );
        assert_eq!(cafe.name, "cafe");
        let inner = got.iter().find(|f| f.path == "cafë/Inner").unwrap();
        assert_eq!((inner.name.as_str(), inner.item_count), ("Inner", 1));
    }
}
