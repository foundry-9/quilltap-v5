//! Folder filing with components — moving or copying items to a folder within
//! one wardrobe, with an outfit's components brought along (or not). v4
//! `lib/wardrobe/folder-filing.ts` (`bbbcd2388`; P4.D309).
//!
//! The folder twin of the cross-wardrobe transfer
//! ([`crate::services::wardrobe_transfers`]): the same all-or-nothing
//! `components` choice, gathered by the same walk
//! ([`crate::wardrobe::collect_container_components`] — only components filed
//! in this wardrobe travel; pieces borrowed from a shared tier stay put).
//!
//!   - **move**, components `none` — the items alone ([`move_items_to_folder`]).
//!   - **move**, components `move` — the items and their components, one
//!     batch, ids kept.
//!   - **move**, components `copy` — fresh copies of the components land in the
//!     folder; the moved outfits are rewired to them, the originals stay put.
//!   - **copy**, components `none` — copies of the items, still naming the
//!     original components.
//!   - **copy**, components `copy` — copies of the items and their components,
//!     the copied outfits rewired to the copied pieces.
//!
//! A copy lives beside its original in the same wardrobe, so it takes the next
//! free `(copy)` title ([`next_copy_title`], as Duplicate does) — a title the
//! wardrobe already holds would make the vault fall back from slug references
//! to UUIDs. It mints a fresh id (a new garment, empty wear ledger) and its
//! pictures are copied under that id. Whenever both an outfit and one of its
//! pieces are copied, the copy names the copy.
//!
//! Like v4, filing is not transactional: a failure part-way through a copy (a
//! cycle refused by the write layer, a picture write failing) leaves the
//! earlier copies and their picture links behind. The only "before any write"
//! guarantees are the copy+move refusal, the folder-path refusal and the
//! missing-id refusal.
//!
//! Sync, on the writer thread, with the item-id mint, the picture `files`-id
//! mint and the one `now` injected (production passes v4 `randomUUID()` and
//! `new Date().toISOString()`'s twins) — the [`crate::services::wardrobe_transfers`]
//! idiom.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;

use crate::db::vault_wardrobe_public::{UpdateManyOptions, WardrobeItemPatch, WardrobePatch};
use crate::db::wardrobe_location::{WardrobeLocation, WardrobeLocationError};
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_item_images::carry::{carry_item_images, CarryArgs, CarryMode};
use crate::services::wardrobe_item_mutations::{
    create_item, item_as_create_body, move_items_to_folder, CreateItemOptions, PreservedIdentity,
};
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe::{collect_container_components, is_composite, next_copy_title};

/// v4 `FileItemsToFolderRequest.action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderFilingAction {
    Move,
    Copy,
}

impl FolderFilingAction {
    pub fn as_str(self) -> &'static str {
        match self {
            FolderFilingAction::Move => "move",
            FolderFilingAction::Copy => "copy",
        }
    }
}

/// v4 `FolderComponentMode` — what travels with an outfit filed into a folder:
/// its components, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderComponentMode {
    Move,
    Copy,
    None,
}

impl FolderComponentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            FolderComponentMode::Move => "move",
            FolderComponentMode::Copy => "copy",
            FolderComponentMode::None => "none",
        }
    }
}

/// v4 `FileItemsToFolderRequest`.
#[derive(Debug, Clone, Copy)]
pub struct FileItemsToFolderRequest<'a> {
    pub action: FolderFilingAction,
    pub item_ids: &'a [String],
    pub folder: &'a str,
    pub components: Option<FolderComponentMode>,
}

/// v4 `FileItemsToFolderResult`.
#[derive(Debug, Clone)]
pub struct FileItemsToFolderResult {
    /// The requested items as they now stand — moved originals, or their copies.
    pub items: Vec<WardrobeItem>,
    /// How many components travelled along (moved or copied) — components
    /// only, never the requested items' own copies.
    pub components_carried: usize,
}

/// The sentence v4's `FolderFilingRequestError` carries for `copy` + `move`
/// (em dash U+2014; the transfers route's refine says the same).
pub const COPY_CANNOT_MOVE_COMPONENTS: &str =
    "Copying an outfit cannot move its components — the original outfit still needs them";

/// A filing failure: v4's `FolderFilingRequestError` (a 400 — a combination
/// that cannot be honoured) or a location error thrown by the steps.
#[derive(Debug)]
pub enum FolderFilingError {
    /// v4 `FolderFilingRequestError` — carries v4's exact message.
    Request(String),
    Location(WardrobeLocationError),
}

impl FolderFilingError {
    /// v4's `error.name` — the class the oracle records.
    pub fn name(&self) -> &'static str {
        match self {
            FolderFilingError::Request(_) => "FolderFilingRequestError",
            FolderFilingError::Location(e) => match e {
                WardrobeLocationError::CharacterArchived { .. } => "CharacterArchivedError",
                WardrobeLocationError::NoLinkedVault { .. } => "Error",
                WardrobeLocationError::Cycle(_) => "WardrobeComponentCycleError",
                WardrobeLocationError::FolderPath(_) => "WardrobeFolderPathError",
                WardrobeLocationError::FolderNotFound { .. } => "WardrobeFolderNotFoundError",
                WardrobeLocationError::FolderConflict { .. } => "WardrobeFolderConflictError",
                WardrobeLocationError::ItemNotInLocation { .. } => "WardrobeItemNotInLocationError",
                WardrobeLocationError::Db(_) => "Error",
            },
        }
    }
}

impl std::fmt::Display for FolderFilingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FolderFilingError::Request(m) => f.write_str(m),
            FolderFilingError::Location(e) => f.write_str(&e.message()),
        }
    }
}

impl std::error::Error for FolderFilingError {}

impl From<WardrobeLocationError> for FolderFilingError {
    fn from(e: WardrobeLocationError) -> Self {
        FolderFilingError::Location(e)
    }
}

impl From<crate::wardrobe_folders::WardrobeFolderPathError> for FolderFilingError {
    fn from(e: crate::wardrobe_folders::WardrobeFolderPathError) -> Self {
        FolderFilingError::Location(WardrobeLocationError::FolderPath(e))
    }
}

impl From<crate::db::DbError> for FolderFilingError {
    fn from(e: crate::db::DbError) -> Self {
        FolderFilingError::Location(WardrobeLocationError::from(e))
    }
}

/// v4 `fileItemsToFolder(repos, userId, location, request)` — move or copy
/// items into one folder of a wardrobe, bringing composites' same-wardrobe
/// components along as asked. Every id must be filed in this wardrobe; a
/// borrowed one refuses the batch before anything is written.
#[allow(clippy::too_many_arguments)]
pub fn file_items_to_folder(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    location: &WardrobeLocation,
    request: &FileItemsToFolderRequest<'_>,
    mint: &mut dyn FnMut() -> String,
    mint_file_id: &mut dyn FnMut() -> String,
    now_iso: &str,
    blob_webp: &dyn WebpTranscoder,
) -> Result<FileItemsToFolderResult, FolderFilingError> {
    let action = request.action;
    let folder = request.folder;
    let component_mode = request.components.unwrap_or(FolderComponentMode::None);
    if action == FolderFilingAction::Copy && component_mode == FolderComponentMode::Move {
        return Err(FolderFilingError::Request(
            COPY_CANNOT_MOVE_COMPONENTS.to_string(),
        ));
    }

    // The plain move needs no plan — it is exactly today's `move-items`, and
    // logs nothing of its own.
    if action == FolderFilingAction::Move && component_mode == FolderComponentMode::None {
        let items = move_items_to_folder(main, mount, location, request.item_ids, folder)?;
        return Ok(FileItemsToFolderResult {
            items,
            components_carried: 0,
        });
    }

    // The folder path is refused (a 400) before the ids are even looked at.
    let target = location.resolve_folder(mount, folder)?;
    let mut seen: HashSet<&str> = HashSet::new();
    let unique_ids: Vec<String> = request
        .item_ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect();
    // Archived items included: they are filed in this wardrobe too, and their
    // titles are taken.
    let wardrobe_items = location.read_items(mount, true)?;
    let by_id: HashMap<&str, &WardrobeItem> = wardrobe_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let missing: Vec<String> = unique_ids
        .iter()
        .filter(|id| !by_id.contains_key(id.as_str()))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(WardrobeLocationError::ItemNotInLocation { item_ids: missing }.into());
    }
    let requested: Vec<&WardrobeItem> = unique_ids.iter().map(|id| by_id[id.as_str()]).collect();

    // Components already in the request are handled as requested items.
    let requested_ids: HashSet<&str> = unique_ids.iter().map(String::as_str).collect();
    let components: Vec<WardrobeItem> = if component_mode == FolderComponentMode::None {
        Vec::new()
    } else {
        let composites: Vec<&WardrobeItem> = requested
            .iter()
            .copied()
            .filter(|item| is_composite(&item.component_item_ids))
            .collect();
        collect_container_components(&composites, &wardrobe_items)
            .into_iter()
            .filter(|item| !requested_ids.contains(item.id.as_str()))
            .collect()
    };

    if action == FolderFilingAction::Move && component_mode == FolderComponentMode::Move {
        let mut ids = unique_ids.clone();
        ids.extend(components.iter().map(|c| c.id.clone()));
        // `target` is already resolved; `move_items_to_folder` resolves it
        // again, as v4's `moveItemsToFolder` does (idempotent).
        let mut moved = move_items_to_folder(main, mount, location, &ids, &target)?;
        log_filed(
            location,
            action,
            component_mode,
            &target,
            unique_ids.len(),
            components.len(),
        );
        // `updateMany` answers in patch order, so the head is the requested items.
        moved.truncate(unique_ids.len());
        return Ok(FileItemsToFolderResult {
            items: moved,
            components_carried: components.len(),
        });
    }

    // Everything that remains mints copies: every component (components `copy`)
    // and, for a copy, every requested item too.
    let to_copy: Vec<&WardrobeItem> = if action == FolderFilingAction::Copy {
        components.iter().chain(requested.iter().copied()).collect()
    } else {
        components.iter().collect()
    };
    let id_map: HashMap<String, String> = to_copy
        .iter()
        .map(|item| (item.id.clone(), mint()))
        .collect();
    // Unmapped ids — borrowed pieces, pieces left behind — keep pointing at
    // the original.
    let remap = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .map(|id| id_map.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect()
    };
    // The whole wardrobe is the title namespace (archived items included):
    // a copy titled like any item here — in any folder — would collide.
    let mut taken_titles: Vec<String> = wardrobe_items.iter().map(|i| i.title.clone()).collect();
    // An archived character's wardrobe refuses here (409) — after the
    // missing-id refusal (400), before any picture is copied.
    let mount_point_id = location.writable_mount_point_id(main)?;
    let now = now_iso;

    // Components first, so an outfit's references resolve the moment it lands.
    let mut copies: HashMap<String, WardrobeItem> = HashMap::new();
    for original in &to_copy {
        let title = next_copy_title(&original.title, &taken_titles);
        taken_titles.push(title.clone());
        let id = id_map[&original.id].clone();
        let carried = carry_item_images(
            main,
            mount,
            &CarryArgs {
                mode: CarryMode::Copy,
                source_item_id: &original.id,
                destination_item_id: &id,
                destination_mount_point_id: &mount_point_id,
                user_id,
            },
            mint_file_id,
            now,
            blob_webp,
        )?;
        let body = item_as_create_body(
            original,
            title,
            target.clone(),
            remap(&original.component_item_ids),
        );
        let preserve = PreservedIdentity {
            id,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            // An archived original yields an archived copy.
            archived_at: Some(original.archived_at.clone().flatten()),
            image_file_id: Some(
                original
                    .image_file_id
                    .clone()
                    .flatten()
                    .filter(|f| !f.is_empty())
                    .and_then(|f| carried.mapped(&f).map(str::to_string)),
            ),
            migrated_from_clothing_record_id: Some(
                original.migrated_from_clothing_record_id.clone().flatten(),
            ),
        };
        let stored = create_item(
            main,
            mount,
            location,
            &body,
            CreateItemOptions {
                lookup: None,
                preserve: Some(&preserve),
            },
        )?;
        copies.insert(original.id.clone(), stored);
    }

    let items: Vec<WardrobeItem> = if action == FolderFilingAction::Copy {
        requested
            .iter()
            .map(|item| copies[&item.id].clone())
            .collect()
    } else {
        // Move with copied components: the outfits move and are rewired to the
        // copies; a requested leaf only changes folder.
        let patches: Vec<WardrobeItemPatch> = requested
            .iter()
            .map(|item| WardrobeItemPatch {
                id: item.id.clone(),
                patch: WardrobePatch {
                    folder: Some(target.clone()),
                    component_item_ids: is_composite(&item.component_item_ids)
                        .then(|| remap(&item.component_item_ids)),
                    ..WardrobePatch::default()
                },
            })
            .collect();
        location.update_many(
            main,
            mount,
            &patches,
            UpdateManyOptions { require_all: true },
        )?
    };

    log_filed(
        location,
        action,
        component_mode,
        &target,
        unique_ids.len(),
        components.len(),
    );
    Ok(FileItemsToFolderResult {
        items,
        components_carried: components.len(),
    })
}

/// v4 `logFiled` — DEBUG `[FolderFiling] Filed wardrobe items into folder` with
/// the RESOLVED folder and the deduped request count.
fn log_filed(
    location: &WardrobeLocation,
    action: FolderFilingAction,
    component_mode: FolderComponentMode,
    folder: &str,
    requested: usize,
    components_carried: usize,
) {
    tracing::debug!(
        scope = location.scope.as_str(),
        mountPointId = %location.mount_point_id,
        action = action.as_str(),
        componentMode = component_mode.as_str(),
        folder = %folder,
        requested = requested,
        componentsCarried = components_carried,
        context = "wardrobe",
        "[FolderFiling] Filed wardrobe items into folder"
    );
}
