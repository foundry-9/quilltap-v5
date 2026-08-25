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
//! It composes the ported repo ops + readers: [`find_by_character_id`],
//! [`read_general_wardrobe`] / [`read_project_wardrobe`], the public writers
//! ([`create_vault_wardrobe_item`] / [`create_project_wardrobe_item`] /
//! [`delete_vault_wardrobe_item`] / [`delete_project_wardrobe_item`]), and the
//! store provisioning ([`ensure_official_store`] + [`ensure_project_wardrobe_folder`]).
//!
//! Move vs copy: `copy` mints a fresh id and `createdAt`/`updatedAt` (`now`);
//! `move` keeps the source id + timestamps and deletes the source after the create.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::collation::locale_compare;
use crate::db::archetype_wardrobe::{
    ensure_group_wardrobe_folder, ensure_project_wardrobe_folder, read_general_wardrobe,
    read_group_wardrobe, read_project_wardrobe, read_shared_wardrobe,
};
use crate::db::characters_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::ensure_official_store::ensure_official_store;
use crate::db::groups::{GroupEntity, GroupsRepository};
use crate::db::projects::{ProjectEntity, ProjectsRepository};
use crate::db::tiered_mount_pool::resolve_group_mount_point_ids_for_character;
use crate::db::vault_wardrobe_public::{
    create_project_wardrobe_item, create_vault_wardrobe_item, delete_project_wardrobe_item,
    delete_vault_wardrobe_item, WardrobePublicError,
};
use crate::db::wardrobe_read::find_by_character_id;
use crate::db::DbError;
use crate::vault_overlay::WardrobeItem;

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
}

/// A transfer failure, mirroring the route's HTTP responses.
#[derive(Debug)]
pub enum TransferError {
    /// v4 `notFound('Wardrobe item')` — the source item didn't resolve.
    NotFound,
    /// v4 `badRequest(msg)` — invalid destination, same source/destination, or an
    /// id collision at the destination.
    BadRequest(String),
    /// v4's explicit `serverError(msg)` arms — the MESSAGE reaches the wire
    /// (unlike [`TransferError::Internal`], which the route's catch collapses
    /// to `'Failed to transfer wardrobe item'`).
    Server(String),
    /// A thrown/unexpected failure — v4's catch → the generic 500 sentence.
    Internal(String),
}

impl From<DbError> for TransferError {
    fn from(e: DbError) -> Self {
        TransferError::Internal(format!("{e:?}"))
    }
}

impl From<WardrobePublicError> for TransferError {
    fn from(e: WardrobePublicError) -> Self {
        TransferError::Internal(format!("{e:?}"))
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

    let name_list = |rows: &[Value], fallback: &str| -> Vec<Value> {
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
    };

    Ok(json!({
        "destinations": {
            "general": { "available": true, "label": "Quilltap General" },
            "projects": name_list(&projects, "Untitled project"),
            "groups": name_list(&groups, "Untitled group"),
            "users": name_list(&characters, "Unnamed user"),
        }
    }))
}

// ── POST: move / copy ────────────────────────────────────────────────────────

/// A source item resolved to its tier (v4 `ResolvedSource`).
struct ResolvedSource {
    scope: SourceScope,
    item: Value,
    character_id: Option<String>,
    mount_point_id: Option<String>,
    /// Every item in the source container (the same list the item was found
    /// in). Used to gather a composite's same-container components so they can
    /// travel with it — components living in *other* tiers stay put.
    container_items: Vec<Value>,
}

/// A transfer source tier (v4 `SourceScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceScope {
    Character,
    Group,
    Project,
    General,
}

impl SourceScope {
    fn as_str(self) -> &'static str {
        match self {
            SourceScope::Character => "character",
            SourceScope::Group => "group",
            SourceScope::Project => "project",
            SourceScope::General => "general",
        }
    }
}

/// A destination resolved to its tier (v4 `ResolvedDestination`).
struct ResolvedDestination {
    scope: DestinationScope,
    character_id: Option<String>,
    mount_point_id: Option<String>,
}

impl DestinationScope {
    fn as_str(self) -> &'static str {
        match self {
            DestinationScope::General => "general",
            DestinationScope::Project => "project",
            DestinationScope::Group => "group",
            DestinationScope::Character => "character",
        }
    }
}

/// v4 `locationKey` — `'general'` for the General tier (singleton), else
/// `'<scope>:<id>'`. Used to reject a source==destination transfer.
fn location_key(scope: &str, id: Option<&str>) -> String {
    if scope == "general" {
        "general".to_string()
    } else {
        format!("{scope}:{}", id.unwrap_or(""))
    }
}

fn item_id_of(item: &Value) -> Option<&str> {
    item.get("id").and_then(Value::as_str)
}

/// v4 `resolveSourceItem` — find the item in the character's personal wardrobe,
/// then the named project, then the character's GROUP stores, then Quilltap
/// General. Ownership: the source character must belong to `user_id`.
///
/// The group tier scans BETWEEN project and General (`8600c83f`) — without it
/// an item moved into a group could not be moved back out, which is half of
/// what made a group garment vanish.
fn resolve_source_item(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    source_character_id: &str,
    source_project_id: Option<&str>,
    item_id: &str,
) -> Result<Option<ResolvedSource>, TransferError> {
    let docs = DocMountDocumentsRepository::new(mount);

    // Ownership gate (v4 reads findById(...).userId).
    let Some(character) = characters_read::find_by_id_raw(main, source_character_id)? else {
        return Ok(None);
    };
    if character.get("userId").and_then(Value::as_str) != Some(user_id) {
        return Ok(None);
    }

    // TRY 1 — personal wardrobe (incl. archived).
    let personal = find_by_character_id(main, &docs, source_character_id, true)?;
    if let Some(item) = personal
        .iter()
        .find(|it| item_id_of(it) == Some(item_id))
        .cloned()
    {
        return Ok(Some(ResolvedSource {
            scope: SourceScope::Character,
            item,
            character_id: Some(source_character_id.to_string()),
            mount_point_id: None,
            container_items: personal,
        }));
    }

    // TRY 2 — the named project's wardrobe.
    if let Some(project_id) = source_project_id {
        let projects = ProjectsRepository::new(main, mount);
        if let Some(project) = projects
            .find_by_id(project_id)
            .map_err(|e| TransferError::Internal(format!("{e:?}")))?
        {
            let name = project
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Project");
            if let Some(ensured) =
                ensure_official_store::<ProjectEntity>(main, mount, project_id, name)?
            {
                let links = DocMountFileLinksRepository::new(mount);
                ensure_project_wardrobe_folder(&links, &ensured.mount_point_id)?;
                let project_items = read_project_wardrobe(&docs, &ensured.mount_point_id, true)?;
                if let Some(item) = project_items
                    .iter()
                    .find(|it| item_id_of(it) == Some(item_id))
                    .cloned()
                {
                    return Ok(Some(ResolvedSource {
                        scope: SourceScope::Project,
                        item,
                        character_id: None,
                        mount_point_id: Some(ensured.mount_point_id),
                        container_items: project_items,
                    }));
                }
            }
        }
    }

    // TRY 3 — the group tier: every store of every group this character belongs
    // to. The source character is the one wearing the item, so their
    // memberships are the right scope — matching how the wearable pool resolves
    // the tier.
    let group_mount_point_ids =
        resolve_group_mount_point_ids_for_character(main, mount, source_character_id);
    for mount_point_id in &group_mount_point_ids {
        let group_items = read_group_wardrobe(&docs, mount_point_id, true)?;
        if let Some(item) = group_items
            .iter()
            .find(|it| item_id_of(it) == Some(item_id))
            .cloned()
        {
            return Ok(Some(ResolvedSource {
                scope: SourceScope::Group,
                item,
                character_id: None,
                mount_point_id: Some(mount_point_id.clone()),
                container_items: group_items,
            }));
        }
    }

    // TRY 4 — Quilltap General.
    let general = read_general_wardrobe(main, &docs, true)?;
    if let Some(item) = general
        .iter()
        .find(|it| item_id_of(it) == Some(item_id))
        .cloned()
    {
        return Ok(Some(ResolvedSource {
            scope: SourceScope::General,
            item,
            character_id: None,
            mount_point_id: None,
            container_items: general,
        }));
    }

    Ok(None)
}

/// v4 `resolveExplicitSource` (`d7263f39`) — resolve the item within an
/// explicitly named source container, no probing. Used when the wardrobe dialog
/// is browsing a shared container directly, so the caller already knows exactly
/// where the item lives.
fn resolve_explicit_source(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    source: &ExplicitSource,
    item_id: &str,
) -> Result<Option<ResolvedSource>, TransferError> {
    let docs = DocMountDocumentsRepository::new(mount);

    if source.scope == SourceScope::General {
        let general = read_general_wardrobe(main, &docs, true)?;
        let Some(item) = general
            .iter()
            .find(|it| item_id_of(it) == Some(item_id))
            .cloned()
        else {
            return Ok(None);
        };
        return Ok(Some(ResolvedSource {
            scope: SourceScope::General,
            item,
            character_id: None,
            mount_point_id: None,
            container_items: general,
        }));
    }

    let Some(id) = source.id.as_deref() else {
        return Ok(None);
    };

    if source.scope == SourceScope::Character {
        let Some(character) = characters_read::find_by_id_raw(main, id)? else {
            return Ok(None);
        };
        if character.get("userId").and_then(Value::as_str) != Some(user_id) {
            return Ok(None);
        }
        let personal = find_by_character_id(main, &docs, id, true)?;
        let Some(item) = personal
            .iter()
            .find(|it| item_id_of(it) == Some(item_id))
            .cloned()
        else {
            return Ok(None);
        };
        return Ok(Some(ResolvedSource {
            scope: SourceScope::Character,
            item,
            character_id: Some(id.to_string()),
            mount_point_id: None,
            container_items: personal,
        }));
    }

    if source.scope == SourceScope::Project {
        let projects = ProjectsRepository::new(main, mount);
        let Some(project) = projects
            .find_by_id(id)
            .map_err(|e| TransferError::Internal(format!("{e:?}")))?
        else {
            return Ok(None);
        };
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Project");
        let Some(ensured) = ensure_official_store::<ProjectEntity>(main, mount, id, name)? else {
            return Ok(None);
        };
        let links = DocMountFileLinksRepository::new(mount);
        ensure_project_wardrobe_folder(&links, &ensured.mount_point_id)?;
        let project_items = read_project_wardrobe(&docs, &ensured.mount_point_id, true)?;
        let Some(item) = project_items
            .iter()
            .find(|it| item_id_of(it) == Some(item_id))
            .cloned()
        else {
            return Ok(None);
        };
        return Ok(Some(ResolvedSource {
            scope: SourceScope::Project,
            item,
            character_id: None,
            mount_point_id: Some(ensured.mount_point_id),
            container_items: project_items,
        }));
    }

    // Group.
    let groups = GroupsRepository::new(main, mount);
    let Some(group) = groups
        .find_by_id(id)
        .map_err(|e| TransferError::Internal(format!("{e:?}")))?
    else {
        return Ok(None);
    };
    let name = group
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("Group");
    let Some(ensured) = ensure_official_store::<GroupEntity>(main, mount, id, name)? else {
        return Ok(None);
    };
    let links = DocMountFileLinksRepository::new(mount);
    ensure_group_wardrobe_folder(&links, &ensured.mount_point_id)?;
    let group_items = read_group_wardrobe(&docs, &ensured.mount_point_id, true)?;
    let Some(item) = group_items
        .iter()
        .find(|it| item_id_of(it) == Some(item_id))
        .cloned()
    else {
        return Ok(None);
    };
    Ok(Some(ResolvedSource {
        scope: SourceScope::Group,
        item,
        character_id: None,
        mount_point_id: Some(ensured.mount_point_id),
        container_items: group_items,
    }))
}

/// v4 `collectContainerComponents` (`f6a10055`) — the transitive closure of a
/// composite's components that live in the same source container. Components
/// from other tiers (e.g. a General archetype bundled into a character outfit)
/// are excluded — they are already shared and stay where they are. Cycles are
/// tolerated via the visited set (the write layer refuses to store them, but
/// old data gets no infinite loop here).
fn collect_container_components(outfit: &Value, container_items: &[Value]) -> Vec<Value> {
    let by_id: HashMap<&str, &Value> = container_items
        .iter()
        .filter_map(|it| item_id_of(it).map(|id| (id, it)))
        .collect();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    if let Some(id) = item_id_of(outfit) {
        visited.insert(id.to_string());
    }
    let mut result: Vec<Value> = Vec::new();
    let mut queue: std::collections::VecDeque<String> = component_ids_of(outfit).into();
    while let Some(id) = queue.pop_front() {
        if visited.contains(&id) {
            continue;
        }
        visited.insert(id.clone());
        let Some(item) = by_id.get(id.as_str()) else {
            continue;
        };
        result.push((*item).clone());
        queue.extend(component_ids_of(item));
    }
    result
}

/// The `componentItemIds` array of a read item (absent/malformed → empty).
fn component_ids_of(item: &Value) -> Vec<String> {
    item.get("componentItemIds")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// v4 `resolveDestination` — validate the destination and (for project/group)
/// provision its official store + `Wardrobe/` folder. `None` = invalid.
fn resolve_destination(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: DestinationScope,
    id: Option<&str>,
) -> Result<Option<ResolvedDestination>, TransferError> {
    match scope {
        DestinationScope::General => Ok(Some(ResolvedDestination {
            scope,
            character_id: None,
            mount_point_id: None,
        })),
        DestinationScope::Character => {
            let Some(id) = id else { return Ok(None) };
            let Some(character) = characters_read::find_by_id_raw(main, id)? else {
                return Ok(None);
            };
            if character.get("userId").and_then(Value::as_str) != Some(user_id) {
                return Ok(None);
            }
            Ok(Some(ResolvedDestination {
                scope,
                character_id: Some(id.to_string()),
                mount_point_id: None,
            }))
        }
        DestinationScope::Project => {
            let Some(id) = id else { return Ok(None) };
            let projects = ProjectsRepository::new(main, mount);
            let Some(project) = projects
                .find_by_id(id)
                .map_err(|e| TransferError::Internal(format!("{e:?}")))?
            else {
                return Ok(None);
            };
            let name = project
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Project");
            let Some(ensured) = ensure_official_store::<ProjectEntity>(main, mount, id, name)?
            else {
                return Ok(None);
            };
            let links = DocMountFileLinksRepository::new(mount);
            ensure_project_wardrobe_folder(&links, &ensured.mount_point_id)?;
            Ok(Some(ResolvedDestination {
                scope,
                character_id: None,
                mount_point_id: Some(ensured.mount_point_id),
            }))
        }
        DestinationScope::Group => {
            let Some(id) = id else { return Ok(None) };
            let groups = GroupsRepository::new(main, mount);
            let Some(group) = groups
                .find_by_id(id)
                .map_err(|e| TransferError::Internal(format!("{e:?}")))?
            else {
                return Ok(None);
            };
            let name = group
                .get("name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("Group");
            let Some(ensured) = ensure_official_store::<GroupEntity>(main, mount, id, name)? else {
                return Ok(None);
            };
            let links = DocMountFileLinksRepository::new(mount);
            ensure_group_wardrobe_folder(&links, &ensured.mount_point_id)?;
            Ok(Some(ResolvedDestination {
                scope,
                character_id: None,
                mount_point_id: Some(ensured.mount_point_id),
            }))
        }
    }
}

/// v4 `readDestinationItems` — every item (incl. archived) at the destination,
/// for the id-collision check.
fn read_destination_items(
    main: &Connection,
    mount: &Connection,
    dest: &ResolvedDestination,
) -> Result<Vec<Value>, TransferError> {
    let docs = DocMountDocumentsRepository::new(mount);
    match dest.scope {
        DestinationScope::General => Ok(read_general_wardrobe(main, &docs, true)?),
        DestinationScope::Character => {
            let cid = dest.character_id.as_deref().unwrap_or_default();
            Ok(find_by_character_id(main, &docs, cid, true)?)
        }
        // Project and group items both live in a mount's `Wardrobe/` folder.
        DestinationScope::Project | DestinationScope::Group => {
            let mp = dest.mount_point_id.as_deref().unwrap_or_default();
            Ok(read_shared_wardrobe(&docs, mp, true)?)
        }
    }
}

/// v4 `createAtDestination` — route the prepared item to the right writer.
fn create_at_destination(
    main: &Connection,
    mount: &Connection,
    dest: &ResolvedDestination,
    item: &WardrobeItem,
) -> Result<WardrobeItem, TransferError> {
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    match dest.scope {
        // General / character route through the public repo (characterId selects
        // the tier: the destination character, or null → Quilltap General).
        DestinationScope::General | DestinationScope::Character => {
            let mut prepared = item.clone();
            prepared.character_id = Some(dest.character_id.clone());
            Ok(create_vault_wardrobe_item(main, &links, &docs, &prepared)?)
        }
        DestinationScope::Project | DestinationScope::Group => {
            let mp = dest.mount_point_id.as_deref().unwrap_or_default();
            Ok(create_project_wardrobe_item(main, &links, &docs, mp, item)?)
        }
    }
}

/// v4 `deleteFromSource` — remove one id from the source container (move only;
/// also used for each travelling component under `components: 'move'`).
fn delete_from_source(
    main: &Connection,
    mount: &Connection,
    source: &ResolvedSource,
    id: &str,
) -> Result<bool, TransferError> {
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    match source.scope {
        // Project and group items both live in a mount's `Wardrobe/` folder
        // rather than a character vault, so both delete by mount point.
        SourceScope::Project | SourceScope::Group => {
            let mp = source.mount_point_id.as_deref().unwrap_or_default();
            Ok(delete_project_wardrobe_item(main, &links, &docs, mp, id)?)
        }
        // Character / general: v4 `repos.wardrobe.delete(id, source.characterId)`
        // (a null characterId locates Quilltap General).
        SourceScope::Character | SourceScope::General => Ok(delete_vault_wardrobe_item(
            main,
            &links,
            &docs,
            id,
            source.character_id.as_deref(),
        )?),
    }
}

/// v4 POST — move or copy one wardrobe item between tiers. `now` is the minted
/// timestamp for a copy's `createdAt`/`updatedAt` (injected for determinism;
/// v4 uses `new Date().toISOString()`).
pub fn transfer_wardrobe_item(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    req: &TransferRequest,
    now: &str,
) -> Result<TransferOutcome, TransferError> {
    // 1. Resolve the source item — explicitly named container first (v4
    //    `body.source ? resolveExplicitSource : resolveSourceItem`).
    let resolved = if let Some(explicit) = &req.source {
        resolve_explicit_source(main, mount, user_id, explicit, &req.item_id)?
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

    // 2. Resolve the destination.
    let Some(destination) = resolve_destination(
        main,
        mount,
        user_id,
        req.destination_scope,
        req.destination_id.as_deref(),
    )?
    else {
        return Err(TransferError::BadRequest("Invalid destination".to_string()));
    };

    // 3. Reject source == destination.
    let source_id = match source.scope {
        SourceScope::Character => source.character_id.as_deref(),
        _ => source.mount_point_id.as_deref(),
    };
    let dest_id = match destination.scope {
        DestinationScope::Character => destination.character_id.as_deref(),
        _ => destination.mount_point_id.as_deref(),
    };
    if location_key(source.scope.as_str(), source_id)
        == location_key(destination.scope.as_str(), dest_id)
    {
        return Err(TransferError::BadRequest(
            "Source and destination are the same".to_string(),
        ));
    }

    let destination_character_id = match destination.scope {
        DestinationScope::Character => destination.character_id.clone(),
        _ => None,
    };

    // 4. The components travelling along: the transitive closure of the
    //    outfit's components that live in the same source container.
    //    All-or-nothing (v4 `f6a10055`).
    let travelling: Vec<Value> = if req.components == ComponentMode::None {
        Vec::new()
    } else {
        collect_container_components(&source.item, &source.container_items)
    };

    // Plan every write up front so id remapping is consistent across the whole
    // set. Moves keep ids; copies mint fresh ones — and every
    // `componentItemIds` reference to a travelling component is rewritten to
    // that component's destination id, so the outfit still points at the very
    // pieces that made the journey with it.
    let mut id_map: HashMap<String, String> = HashMap::new();
    for component in &travelling {
        let cid = item_id_of(component).unwrap_or_default().to_string();
        let mapped = if req.components == ComponentMode::Copy {
            uuid::Uuid::new_v4().to_string()
        } else {
            cid.clone()
        };
        id_map.insert(cid, mapped);
    }
    let remap = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .map(|id| id_map.get(id).cloned().unwrap_or_else(|| id.clone()))
            .collect()
    };

    let planned_components: Vec<WardrobeItem> = travelling
        .iter()
        .map(|component| {
            let mut planned = WardrobeItem::from_read_value(component);
            planned.id = id_map
                .get(&planned.id)
                .cloned()
                .unwrap_or_else(|| planned.id.clone());
            planned.character_id = Some(destination_character_id.clone());
            planned.component_item_ids = remap(&planned.component_item_ids);
            if req.components == ComponentMode::Copy {
                planned.created_at = now.to_string();
                planned.updated_at = now.to_string();
            }
            planned
        })
        .collect();

    // 5. Prepare the moved/copied outfit itself.
    let mut next_item = WardrobeItem::from_read_value(&source.item);
    if req.action == TransferAction::Copy {
        next_item.id = uuid::Uuid::new_v4().to_string();
        next_item.created_at = now.to_string();
        next_item.updated_at = now.to_string();
    }
    next_item.character_id = Some(destination_character_id.clone());
    next_item.component_item_ids = remap(&next_item.component_item_ids);

    // 6. Refuse the whole transfer before writing anything if any planned id
    //    is already taken at the destination — all-or-nothing means no
    //    half-landed outfits. (v4 renders the planned item's TITLE inside
    //    "the ID of" — the quirk is wire payload, keep it.)
    let destination_items = read_destination_items(main, mount, &destination)?;
    let destination_ids: std::collections::HashSet<&str> = destination_items
        .iter()
        .filter_map(|it| item_id_of(it))
        .collect();
    for planned in std::iter::once(&next_item).chain(planned_components.iter()) {
        if destination_ids.contains(planned.id.as_str()) {
            return Err(TransferError::BadRequest(format!(
                "An item with the ID of \"{}\" already exists at the destination",
                planned.title
            )));
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
                let cid = item_id_of(component).unwrap_or_default();
                if !delete_from_source(main, mount, &source, cid)? {
                    return Err(TransferError::Server(
                        "Failed to remove a component from source after move".to_string(),
                    ));
                }
            }
        }
        let iid = item_id_of(&source.item).unwrap_or_default();
        if !delete_from_source(main, mount, &source, iid)? {
            return Err(TransferError::Server(
                "Failed to remove item from source after move".to_string(),
            ));
        }
    }

    // 9. Post-write verification: read the outfit BACK from the destination
    //    and check that its component references survived the storage
    //    round-trip exactly as planned — the vault serializes references as
    //    title slugs, so a subtle resolution bug shows up here, not in the
    //    pre-projection value `create_at_destination` returned. Anything
    //    planned-but-absent from the read-back list is reported.
    let after_items = read_destination_items(main, mount, &destination)?;
    let after_outfit = after_items
        .iter()
        .find(|it| item_id_of(it) == Some(stored.id.as_str()));
    let read_back_ids: std::collections::HashSet<String> = after_outfit
        .map(component_ids_of)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let unresolved_component_ids: Vec<String> = next_item
        .component_item_ids
        .iter()
        .filter(|id| !read_back_ids.contains(*id))
        .cloned()
        .collect();
    if after_outfit.is_none() || !unresolved_component_ids.is_empty() {
        tracing::error!(
            user_id,
            outfit_id = %stored.id,
            outfit_found_at_destination = after_outfit.is_some(),
            planned_component_ids = ?next_item.component_item_ids,
            read_back_component_ids = ?after_outfit.map(component_ids_of).unwrap_or_default(),
            unresolved_component_ids = ?unresolved_component_ids,
            destination_scope = destination.scope.as_str(),
            destination_mount_point_id = ?destination.mount_point_id,
            "[WardrobeTransfers v1] Transferred outfit did not read back with its planned component references"
        );
    }

    Ok(TransferOutcome {
        wardrobe_item: serde_json::to_value(&stored).unwrap_or(Value::Null),
        action: req.action,
        components_transferred: planned_components.len(),
        unresolved_component_ids,
    })
}

/// The string form of the outcome action (for a response/log).
pub fn action_str(action: TransferAction) -> &'static str {
    action.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn location_key_general_is_singleton() {
        // General ignores the id (there is one General store).
        assert_eq!(location_key("general", None), "general");
        assert_eq!(location_key("general", Some("anything")), "general");
    }

    #[test]
    fn location_key_scopes_by_id() {
        assert_eq!(location_key("character", Some("c1")), "character:c1");
        assert_eq!(location_key("project", Some("p1")), "project:p1");
        // A null id degrades to the empty suffix (v4 `id ?? ''`).
        assert_eq!(location_key("project", None), "project:");
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
