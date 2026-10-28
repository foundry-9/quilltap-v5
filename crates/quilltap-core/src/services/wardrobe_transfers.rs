//! Wardrobe transfers (v4 `app/api/v1/wardrobe/transfers/route.ts`) — moving or
//! copying one wardrobe item between the four tiers (character vault / Quilltap
//! General / project / group). Ported as a core service function; the HTTP
//! transport shim is Phase-4.
//!
//! Two entry points mirror the route's `GET`/`POST`:
//!
//!   - [`enumerate_destinations`] — the destination options (General + the user's
//!     projects/groups/characters, each name-sorted).
//!   - [`transfer_wardrobe_item`] — resolve source (explicit container or
//!     character-probed) → resolve destination → guard same-location → plan the
//!     moved/copied item + its travelling components (`f6a10055`: the transitive
//!     closure of same-container components, all-or-nothing, ids remapped on
//!     copy) → collision-check EVERY planned id → components land first, then
//!     the outfit → (move only) delete travelling components + the item →
//!     post-write read-back verification (`unresolvedComponentIds`).
//!
//! Since `9ecebf986` every tier is a wardrobe LOCATION
//! ([`resolve_wardrobe_location`]): the source is probed character → GROUPS
//! (strongest first) → project → General WITHOUT provisioning (bug 192 — v5
//! probed the project before the groups and provisioned its store on the
//! probe), only the destination is resolved with `ensure: true`, items land
//! through the keystone's `create_item` with their identity preserved (the
//! destination folder created as needed), and a move deletes through
//! `location.delete`. Every planned item carries its `folder` (`4a38222e6`),
//! normalised — a hand-made folder the rules refuse lands at the root.
//!
//! Move vs copy: `copy` mints a fresh id and `createdAt`/`updatedAt` (`now`);
//! `move` keeps the source id + timestamps and deletes the source after the create.
//!
//! P4.D263 (v4 `7c8572869`, #82): pictures (`Wardrobe/images/<itemId>/` in the
//! source mount) travel with every transferred item. A move re-links them into
//! the destination mount at the same path and re-points their `files` rows,
//! dropping the source links once the source item is gone; a copy links them
//! under the copy's new id with fresh `files` rows, and the copy's
//! `imageFileId` points at its own copy (a picture that cannot be carried
//! nulls the pointer). A move writes to the source too, so its writable mount
//! is resolved BEFORE anything is written: an archived source character
//! refuses there (v4's 409 `An archived character's wardrobe cannot be
//! changed`), as does an archived destination — a COPY out of an archived
//! source still succeeds.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::collation::locale_compare;
use crate::db::characters_read;
use crate::db::groups::GroupsRepository;
use crate::db::projects::ProjectsRepository;
use crate::db::vault_wardrobe_public::WardrobePublicError;
use crate::db::wardrobe_location::{
    group_locations_for_character, resolve_wardrobe_location, ResolveWardrobeLocationOptions,
    WardrobeLocation, WardrobeLocationError, WardrobeScope,
};
use crate::db::DbError;
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::wardrobe_item_images::carry::{
    carry_item_images, commit_moved_images, CarryArgs, CarryMode, PendingImageMove,
};
use crate::services::wardrobe_item_mutations::{
    create_item, item_as_create_body, CreateItemOptions, PreservedIdentity,
};
use crate::vault_overlay::WardrobeItem;
use crate::wardrobe_folders::normalize_wardrobe_folder_path_or_root;

/// v4's `conflict(...)` sentence for a `CharacterArchivedError` (the route's
/// catch, `transfers/route.ts:501-503`).
const ARCHIVED_CONFLICT: &str = "An archived character's wardrobe cannot be changed";

/// Move or copy (v4 `TransferAction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferAction {
    Move,
    Copy,
}

impl TransferAction {
    fn as_str(self) -> &'static str {
        match self {
            TransferAction::Move => "move",
            TransferAction::Copy => "copy",
        }
    }
}

/// A transfer destination tier (v4 `DestinationScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestinationScope {
    General,
    Project,
    Group,
    Character,
}

/// What travels with a composite: its same-container components, or nothing
/// (v4 `ComponentMode`, `f6a10055`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComponentMode {
    Move,
    Copy,
    #[default]
    None,
}

impl ComponentMode {
    /// The wire / log spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ComponentMode::Move => "move",
            ComponentMode::Copy => "copy",
            ComponentMode::None => "none",
        }
    }
}

/// An explicitly named source container (v4 the `source` schema field,
/// `d7263f39`) — used when the dialog browses a shared container directly, so
/// the home tier is already known and nothing is probed.
#[derive(Debug, Clone)]
pub struct ExplicitSource {
    pub scope: SourceScope,
    pub id: Option<String>,
}

/// The parsed transfer request (v4 `transferRequestSchema`). `destination_id` is
/// required for every scope except `General`. At least one of
/// `source_character_id` / `source` is present (the schema refine guarantees it).
#[derive(Debug, Clone)]
pub struct TransferRequest {
    pub action: TransferAction,
    pub item_id: String,
    pub source_character_id: Option<String>,
    pub source_project_id: Option<String>,
    pub source: Option<ExplicitSource>,
    pub destination_scope: DestinationScope,
    pub destination_id: Option<String>,
    pub components: ComponentMode,
}

/// A successful transfer (v4's `{ wardrobeItem, action, componentsTransferred,
/// ...(unresolvedComponentIds when non-empty) }`).
pub struct TransferOutcome {
    pub wardrobe_item: Value,
    pub action: TransferAction,
    pub components_transferred: usize,
    /// Planned component references the post-write read-back could NOT resolve
    /// at the destination (empty = the key is omitted from the response).
    pub unresolved_component_ids: Vec<String>,
    /// The read-back ERROR's payload, when the read-back failed.
    pub read_back_failure: Option<ReadBackFailure>,
    /// The success INFO's payload.
    pub transferred: TransferredLine,
}

/// A transfer failure, mirroring the route's HTTP responses.
#[derive(Debug)]
pub enum TransferError {
    /// v4 `notFound('Wardrobe item')` — the source item didn't resolve.
    NotFound,
    /// v4 `badRequest(msg)` — invalid destination, same source/destination, or an
    /// id collision at the destination.
    BadRequest(String),
    /// v4 `conflict(...)` — a `CharacterArchivedError` out of the source /
    /// destination mount resolution or a vault writer (P4.D263).
    Conflict(String),
    /// v4's explicit `serverError(msg)` arms — the MESSAGE reaches the wire
    /// (unlike [`TransferError::Internal`], which the route's catch collapses
    /// to `'Failed to transfer wardrobe item'`).
    Server(String),
    /// A thrown/unexpected failure — v4's catch → the generic 500 sentence.
    Internal(String),
}

impl From<DbError> for TransferError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::CharacterArchived { .. } => {
                TransferError::Conflict(ARCHIVED_CONFLICT.to_string())
            }
            e => TransferError::Internal(format!("{e:?}")),
        }
    }
}

impl From<WardrobePublicError> for TransferError {
    fn from(e: WardrobePublicError) -> Self {
        match e {
            WardrobePublicError::Db(e) => e.into(),
            e => TransferError::Internal(format!("{e:?}")),
        }
    }
}

// ── GET: destination enumeration ─────────────────────────────────────────────

/// v4 GET — the destination options for the transfer UI: General (always
/// available), plus the user's projects, groups, and characters, each sorted by
/// name (locale-aware, matching v4's `localeCompare`). Falsy names fall back to
/// v4's placeholders.
pub fn enumerate_destinations(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
) -> Result<Value, TransferError> {
    let projects = ProjectsRepository::new(main, mount)
        .find_all()
        .map_err(|e| TransferError::Internal(format!("{e:?}")))?;
    let groups = GroupsRepository::new(main, mount)
        .find_all()
        .map_err(|e| TransferError::Internal(format!("{e:?}")))?;
    let characters = characters_read::find_by_user_id(main, mount, user_id)?;
    Ok(destinations_body(&projects, &groups, &characters))
}

/// v4 GET's success body over the three lists it read — ONE naming / sorting
/// rule for both the sound enumeration and the degraded-mount arm
/// (`api::wardrobe`, P4.166): each entity as `{ id, name }` where a falsy name
/// falls back to v4's placeholder (`project.name || 'Untitled project'`), the
/// list sorted by `name.localeCompare`.
pub fn destinations_body(projects: &[Value], groups: &[Value], users: &[Value]) -> Value {
    json!({
        "destinations": {
            "general": { "available": true, "label": "Quilltap General" },
            "projects": name_list(projects, "Untitled project"),
            "groups": name_list(groups, "Untitled group"),
            "users": name_list(users, "Unnamed user"),
        }
    })
}

/// `rows.map(r => ({ id: r.id, name: r.name || fallback })).sort((a, b) =>
/// a.name.localeCompare(b.name))`.
fn name_list(rows: &[Value], fallback: &str) -> Vec<Value> {
    let mut named: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            let id = r
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let name = r
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(fallback)
                .to_string();
            (id, name)
        })
        .collect();
    named.sort_by(|a, b| locale_compare(&a.1, &b.1));
    named
        .into_iter()
        .map(|(id, name)| json!({ "id": id, "name": name }))
        .collect()
}

// ── POST: move / copy ────────────────────────────────────────────────────────

/// A source item resolved to its tier (v4 `ResolvedSource`).
struct ResolvedSource {
    location: WardrobeLocation,
    item: WardrobeItem,
    /// Every item in the source folder (the same list the item was found in).
    /// Used to gather a composite's same-folder components so they can travel
    /// with it — components living in *other* tiers stay put.
    container_items: Vec<WardrobeItem>,
}

/// A transfer source tier (v4 `WardrobeScopeSchema` on `source.scope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceScope {
    Character,
    Group,
    Project,
    General,
}

impl SourceScope {
    fn wardrobe_scope(self) -> WardrobeScope {
        match self {
            SourceScope::Character => WardrobeScope::Character,
            SourceScope::Group => WardrobeScope::Group,
            SourceScope::Project => WardrobeScope::Project,
            SourceScope::General => WardrobeScope::General,
        }
    }
}

impl DestinationScope {
    fn wardrobe_scope(self) -> WardrobeScope {
        match self {
            DestinationScope::General => WardrobeScope::General,
            DestinationScope::Project => WardrobeScope::Project,
            DestinationScope::Group => WardrobeScope::Group,
            DestinationScope::Character => WardrobeScope::Character,
        }
    }
}

/// A location error as the route's catch answers it: an archived character is
/// the 409; anything else rethrows into `Failed to transfer wardrobe item`.
impl From<WardrobeLocationError> for TransferError {
    fn from(e: WardrobeLocationError) -> Self {
        match e {
            WardrobeLocationError::CharacterArchived { .. } => {
                TransferError::Conflict(ARCHIVED_CONFLICT.to_string())
            }
            WardrobeLocationError::Db(e) => e.into(),
            e => TransferError::Internal(e.message()),
        }
    }
}

/// v4 `findInLocation(location, itemId)` — the item among the folder's items
/// (archived included), with the folder's items.
fn find_in_location(
    mount: &Connection,
    location: WardrobeLocation,
    item_id: &str,
) -> Result<Option<ResolvedSource>, TransferError> {
    let container_items = location.read_items(mount, true)?;
    let Some(item) = container_items.iter().find(|i| i.id == item_id).cloned() else {
        return Ok(None);
    };
    Ok(Some(ResolvedSource {
        location,
        item,
        container_items,
    }))
}

/// v4 `resolveWardrobeLocation(scope, id, repos, userId)` — a READ (never
/// provisions).
fn read_location(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: WardrobeScope,
    id: Option<&str>,
) -> Result<Option<WardrobeLocation>, TransferError> {
    Ok(resolve_wardrobe_location(
        main,
        mount,
        scope,
        id,
        user_id,
        ResolveWardrobeLocationOptions { ensure: false },
    )?)
}

/// v4 `resolveSourceItem` (bug 192, `9ecebf986`) — find the item in the
/// character's vault, then the character's GROUP stores (the LATER group mount
/// first — later mounts shadow earlier ones, so probe strongest first), then
/// the named project, then Quilltap General. Probing NEVER provisions: a
/// project with no store yet is skipped, not created. The ownership gate
/// (the character must be `user_id`'s) is the location's (R-E).
fn resolve_source_item(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    source_character_id: &str,
    source_project_id: Option<&str>,
    item_id: &str,
) -> Result<Option<ResolvedSource>, TransferError> {
    let Some(personal) = read_location(
        main,
        mount,
        user_id,
        WardrobeScope::Character,
        Some(source_character_id),
    )?
    else {
        return Ok(None);
    };
    if let Some(own) = find_in_location(mount, personal, item_id)? {
        return Ok(Some(own));
    }

    // The source character is the one wearing the item, so their memberships
    // are the right scope — matching how the wearable pool resolves the tier.
    // Later group mounts shadow earlier ones, so probe strongest first.
    let mut groups = group_locations_for_character(main, mount, source_character_id);
    groups.reverse();
    for group in groups {
        if let Some(hit) = find_in_location(mount, group, item_id)? {
            return Ok(Some(hit));
        }
    }

    // v4 `if (sourceProjectId)` — an empty string is falsy too.
    if let Some(project_id) = source_project_id.filter(|s| !s.is_empty()) {
        if let Some(project) = read_location(
            main,
            mount,
            user_id,
            WardrobeScope::Project,
            Some(project_id),
        )? {
            if let Some(hit) = find_in_location(mount, project, item_id)? {
                return Ok(Some(hit));
            }
        }
    }

    match read_location(main, mount, user_id, WardrobeScope::General, None)? {
        Some(general) => find_in_location(mount, general, item_id),
        None => Ok(None),
    }
}

/// v4 `createAtDestination(destination, planned)` — `createItem(destination,
/// planned, { preserve: { id, createdAt, updatedAt, archivedAt, imageFileId,
/// migratedFromClothingRecordId } })`: the planned item rides as a create body
/// (so the destination folder is resolved — created as needed, an existing
/// one in any casing used as stored), with no lookup (a composite keeps its
/// planned `types`), its identity and history preserved.
fn create_at_destination(
    main: &Connection,
    mount: &Connection,
    destination: &WardrobeLocation,
    planned: &WardrobeItem,
) -> Result<WardrobeItem, TransferError> {
    let body = item_as_create_body(
        planned,
        planned.title.clone(),
        planned.folder.clone(),
        planned.component_item_ids.clone(),
    );
    let preserve = PreservedIdentity {
        id: planned.id.clone(),
        created_at: planned.created_at.clone(),
        updated_at: planned.updated_at.clone(),
        archived_at: Some(planned.archived_at.clone().flatten()),
        image_file_id: Some(planned.image_file_id.clone().flatten()),
        migrated_from_clothing_record_id: Some(
            planned.migrated_from_clothing_record_id.clone().flatten(),
        ),
    };
    Ok(create_item(
        main,
        mount,
        destination,
        &body,
        CreateItemOptions {
            lookup: None,
            preserve: Some(&preserve),
        },
    )?)
}

/// v4 POST — move or copy one wardrobe item between tiers. `now` is the minted
/// timestamp for a copy's `createdAt`/`updatedAt` (injected for determinism;
/// v4 uses `new Date().toISOString()`) and for the carried pictures' rows;
/// `blob_webp` is the blob codec the picture links write through.
pub fn transfer_wardrobe_item(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    req: &TransferRequest,
    now: &str,
    blob_webp: &dyn WebpTranscoder,
) -> Result<TransferOutcome, TransferError> {
    // 1. Resolve the source item — an explicitly named folder (no probing, no
    //    provisioning), else the character-view probe.
    let resolved = if let Some(explicit) = &req.source {
        match read_location(
            main,
            mount,
            user_id,
            explicit.scope.wardrobe_scope(),
            explicit.id.as_deref(),
        )? {
            Some(location) => find_in_location(mount, location, &req.item_id)?,
            None => None,
        }
    } else {
        resolve_source_item(
            main,
            mount,
            user_id,
            req.source_character_id.as_deref().unwrap_or_default(),
            req.source_project_id.as_deref(),
            &req.item_id,
        )?
    };
    let Some(source) = resolved else {
        return Err(TransferError::NotFound);
    };

    // 2. Resolve the destination — the ONE place a store is provisioned.
    let Some(destination) = resolve_wardrobe_location(
        main,
        mount,
        req.destination_scope.wardrobe_scope(),
        req.destination_id.as_deref(),
        user_id,
        ResolveWardrobeLocationOptions { ensure: true },
    )?
    else {
        return Err(TransferError::BadRequest("Invalid destination".to_string()));
    };

    // 3. Reject source == destination (`scope:mountPointId`).
    if source.location.key() == destination.key() {
        return Err(TransferError::BadRequest(
            "Source and destination are the same".to_string(),
        ));
    }

    let destination_character_id = destination.character_id.clone();

    // 4. The components travelling along: the transitive closure of the
    //    outfit's components that live in the same source folder.
    //    All-or-nothing (v4 `f6a10055`).
    let travelling: Vec<WardrobeItem> = if req.components == ComponentMode::None {
        Vec::new()
    } else {
        // v4 `bbbcd2388`: the array form with ONE composite
        // (`transfers/route.ts:256`) — the same result as the old one-outfit walk.
        crate::wardrobe::collect_container_components(
            std::slice::from_ref(&&source.item),
            &source.container_items,
        )
    };

    // Plan every write up front so id remapping is consistent across the whole
    // set. Moves keep ids; copies mint fresh ones — and every
    // `componentItemIds` reference to a travelling component is rewritten to
    // that component's destination id, so the outfit still points at the very
    // pieces that made the journey with it. Every planned item keeps its
    // folder path (`4a38222e6`): a hand-made folder the rules cannot normalise
    // lands at the ROOT rather than failing the transfer.
    let mut id_map: HashMap<String, String> = HashMap::new();
    for component in &travelling {
        let mapped = if req.components == ComponentMode::Copy {
            uuid::Uuid::new_v4().to_string()
        } else {
            component.id.clone()
        };
        id_map.insert(component.id.clone(), mapped);
    }
    let remap = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .map(|id| id_map.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect()
    };

    let mut planned_components: Vec<WardrobeItem> = travelling
        .iter()
        .map(|component| {
            let mut planned = component.clone();
            planned.id = id_map
                .get(&component.id)
                .cloned()
                .unwrap_or_else(|| component.id.clone());
            planned.character_id = Some(destination_character_id.clone());
            planned.component_item_ids = remap(&component.component_item_ids);
            planned.folder = normalize_wardrobe_folder_path_or_root(Some(&component.folder));
            if req.components == ComponentMode::Copy {
                planned.created_at = now.to_string();
                planned.updated_at = now.to_string();
            }
            planned
        })
        .collect();

    // 5. Prepare the moved/copied outfit itself.
    let mut next_item = source.item.clone();
    if req.action == TransferAction::Copy {
        next_item.id = uuid::Uuid::new_v4().to_string();
        next_item.created_at = now.to_string();
        next_item.updated_at = now.to_string();
    }
    next_item.character_id = Some(destination_character_id.clone());
    next_item.component_item_ids = remap(&source.item.component_item_ids);
    next_item.folder = normalize_wardrobe_folder_path_or_root(Some(&source.item.folder));

    // 6. Refuse the whole transfer before writing anything if any planned id
    //    is already taken at the destination — all-or-nothing means no
    //    half-landed outfits. (v4 renders the planned item's TITLE inside
    //    "the ID of" — the quirk is wire payload, keep it.)
    let destination_items = destination.read_items(mount, true)?;
    let destination_ids: std::collections::HashSet<&str> =
        destination_items.iter().map(|it| it.id.as_str()).collect();
    for planned in std::iter::once(&next_item).chain(planned_components.iter()) {
        if destination_ids.contains(planned.id.as_str()) {
            return Err(TransferError::BadRequest(format!(
                "An item with the ID of \"{}\" already exists at the destination",
                planned.title
            )));
        }
    }

    // 6b. A move writes to the source too (its item and its picture links
    //     go), so its writable mount is resolved before anything is written:
    //     an archived source character refuses here (the tombstone), not
    //     half-way through.
    if req.action == TransferAction::Move || req.components == ComponentMode::Move {
        source.location.writable_mount_point_id(main)?;
    }

    // 6c. Pictures are linked at the destination before the items land, so a
    //     landed item's `imageFileId` never dangles. A copy's pointer is
    //     rewritten to its own copied file; a move's rows are re-pointed only
    //     after the source item is gone (`commit_moved_images`), so a failure
    //     before then leaves the source whole.
    let destination_mount_point_id = destination.writable_mount_point_id(main)?;
    let component_mode = if req.components == ComponentMode::Copy {
        CarryMode::Copy
    } else {
        CarryMode::Move
    };
    let item_mode = match req.action {
        TransferAction::Copy => CarryMode::Copy,
        TransferAction::Move => CarryMode::Move,
    };
    let mut travellers: Vec<(String, &mut WardrobeItem, CarryMode)> = travelling
        .iter()
        .map(|c| c.id.clone())
        .zip(planned_components.iter_mut())
        .map(|(original_id, planned)| (original_id, planned, component_mode))
        .collect();
    travellers.push((source.item.id.clone(), &mut next_item, item_mode));
    let mut pending_image_moves: Vec<(String, PendingImageMove)> = Vec::new();
    let mut mint_file_id = || uuid::Uuid::new_v4().to_string();
    for (original_id, planned, mode) in travellers {
        let carried = carry_item_images(
            main,
            mount,
            &CarryArgs {
                mode,
                source_item_id: &original_id,
                destination_item_id: &planned.id,
                destination_mount_point_id: &destination_mount_point_id,
                user_id,
                images_folder: crate::services::wardrobe_image_paths::WARDROBE_IMAGES_FOLDER,
            },
            &mut mint_file_id,
            now,
            blob_webp,
        )?;
        // The planned item still carries the ORIGINAL's pointer here.
        let current = planned.image_file_id.clone().flatten();
        planned.image_file_id = Some(
            current
                .as_deref()
                .and_then(|id| carried.mapped(id))
                .map(str::to_string),
        );
        if mode == CarryMode::Move {
            pending_image_moves.push((original_id, carried.pending_move));
        }
    }

    // 7. Components land first so the outfit's references resolve the moment
    //    it arrives; the write layer tolerates missing components, but there
    //    is no reason to create that window.
    for planned in &planned_components {
        create_at_destination(main, mount, &destination, planned)?;
    }
    let stored = create_at_destination(main, mount, &destination, &next_item)?;

    // 8. Delete from source (move only). `components: 'copy'` leaves the
    //    originals at the source (they were duplicated, not relocated); only
    //    `'move'` removes them.
    if req.action == TransferAction::Move {
        if req.components == ComponentMode::Move {
            for component in &travelling {
                if !source.location.delete(main, mount, &component.id)? {
                    return Err(TransferError::Server(
                        "Failed to remove a component from source after move".to_string(),
                    ));
                }
            }
        }
        if !source.location.delete(main, mount, &source.item.id)? {
            return Err(TransferError::Server(
                "Failed to remove item from source after move".to_string(),
            ));
        }
    }

    // 8b. The moved items' source-side picture links go once their items have.
    for (item_id, pending) in &pending_image_moves {
        commit_moved_images(
            main,
            mount,
            item_id,
            pending,
            now,
            crate::services::wardrobe_image_paths::WARDROBE_IMAGES_FOLDER,
        );
    }

    // 9. Post-write verification: read the outfit BACK from the destination
    //    and check that its component references survived the storage
    //    round-trip exactly as planned — the vault serializes references as
    //    title slugs, so a subtle resolution bug shows up here, not in the
    //    pre-projection value `create_at_destination` returned. Anything
    //    planned-but-absent from the read-back list is reported.
    let after_items = destination.read_items(mount, true)?;
    let after_outfit = after_items.iter().find(|it| it.id == stored.id);
    let read_back: Vec<String> = after_outfit
        .map(|o| o.component_item_ids.clone())
        .unwrap_or_default();
    let read_back_ids: std::collections::HashSet<&str> =
        read_back.iter().map(String::as_str).collect();
    let unresolved_component_ids: Vec<String> = next_item
        .component_item_ids
        .iter()
        .filter(|id| !read_back_ids.contains(id.as_str()))
        .cloned()
        .collect();
    let read_back_failure =
        (after_outfit.is_none() || !unresolved_component_ids.is_empty()).then(|| ReadBackFailure {
            outfit_id: stored.id.clone(),
            outfit_found_at_destination: after_outfit.is_some(),
            planned_component_ids: next_item.component_item_ids.clone(),
            read_back_component_ids: read_back.clone(),
            destination_scope: destination.scope.as_str(),
            destination_mount_point_id: destination.mount_point_id.clone(),
        });

    Ok(TransferOutcome {
        wardrobe_item: crate::api::wardrobe_route_factory::item_create_value(&stored),
        action: req.action,
        components_transferred: planned_components.len(),
        unresolved_component_ids,
        read_back_failure,
        transferred: TransferredLine {
            component_mode: req.components.as_str(),
            item_id: source.item.id.clone(),
            result_item_id: stored.id.clone(),
            folder: stored.folder.clone(),
            source_scope: source.location.scope.as_str(),
            destination_scope: destination.scope.as_str(),
            source_character_id: source.location.character_id.clone(),
            destination_character_id: destination.character_id.clone(),
            source_mount_point_id: source.location.mount_point_id.clone(),
            destination_mount_point_id: destination.mount_point_id.clone(),
        },
    })
}

/// The post-write read-back's ERROR payload (v4 `[WardrobeTransfers v1]
/// Transferred outfit did not read back with its planned component
/// references`), logged by the route on the CALLING thread.
#[derive(Debug, Clone)]
pub struct ReadBackFailure {
    pub outfit_id: String,
    pub outfit_found_at_destination: bool,
    pub planned_component_ids: Vec<String>,
    pub read_back_component_ids: Vec<String>,
    pub destination_scope: &'static str,
    pub destination_mount_point_id: String,
}

/// The success INFO's payload (v4 `[WardrobeTransfers v1] Wardrobe item
/// transferred`), logged by the route on the CALLING thread.
#[derive(Debug, Clone)]
pub struct TransferredLine {
    pub component_mode: &'static str,
    pub item_id: String,
    pub result_item_id: String,
    /// `stored.folder ?? ''` (`4a38222e6`).
    pub folder: String,
    pub source_scope: &'static str,
    pub destination_scope: &'static str,
    pub source_character_id: Option<String>,
    pub destination_character_id: Option<String>,
    pub source_mount_point_id: String,
    pub destination_mount_point_id: String,
}

/// The string form of the outcome action (for a response/log).
pub fn action_str(action: TransferAction) -> &'static str {
    action.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `9ecebf986`: the same-location test keys on `scope:mountPointId`
    /// (`db::wardrobe_location::location_key`) — the OLD `general` /
    /// `scope:characterId|mountPointId` keying and its two pins are retired.
    #[test]
    fn the_same_location_key_is_the_locations() {
        use crate::db::wardrobe_location::location_key;
        assert_eq!(location_key(WardrobeScope::General, "g-mp"), "general:g-mp");
        assert_eq!(
            location_key(WardrobeScope::Character, "v-mp"),
            "character:v-mp"
        );
        assert_ne!(
            location_key(WardrobeScope::Project, "mp"),
            location_key(WardrobeScope::Group, "mp")
        );
    }

    #[test]
    fn copy_move_id_and_timestamp_semantics() {
        // A read item (from the wardrobe read shape).
        let read = json!({
            "id": "src-1", "characterId": "owner", "title": "Coat",
            "description": null, "imagePrompt": null, "types": ["top"],
            "componentItemIds": [], "appropriateness": null, "isDefault": false,
            "replace": false, "migratedFromClothingRecordId": null, "archivedAt": null,
            "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-02T00:00:00.000Z"
        });
        let item = WardrobeItem::from_read_value(&read);
        // Round-trip preserves every field verbatim (a MOVE keeps id + timestamps).
        assert_eq!(item.id, "src-1");
        assert_eq!(item.created_at, "2026-01-01T00:00:00.000Z");
        assert_eq!(item.updated_at, "2026-01-02T00:00:00.000Z");
        assert_eq!(item.character_id, Some(Some("owner".to_string())));
        assert_eq!(item.types, vec!["top".to_string()]);
        assert!(!item.is_default);
    }
}
