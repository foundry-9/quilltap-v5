//! Wardrobe transfers (v4 `app/api/v1/wardrobe/transfers/route.ts`) — moving or
//! copying one wardrobe item between the four tiers (character vault / Quilltap
//! General / project / group). Ported as a core service function; the HTTP
//! transport shim is Phase-4.
//!
//! Two entry points mirror the route's `GET`/`POST`:
//!
//!   - [`enumerate_destinations`] — the destination options (General + the user's
//!     projects/groups/characters, each name-sorted).
//!   - [`transfer_wardrobe_item`] — resolve source → resolve destination → guard
//!     same-location → prepare the moved/copied item → collision-check → create at
//!     destination → (move only) delete from source.
//!
//! It composes the ported repo ops + readers: [`find_by_character_id`],
//! [`read_general_wardrobe`] / [`read_project_wardrobe`], the public writers
//! ([`create_vault_wardrobe_item`] / [`create_project_wardrobe_item`] /
//! [`delete_vault_wardrobe_item`] / [`delete_project_wardrobe_item`]), and the
//! store provisioning ([`ensure_official_store`] + [`ensure_project_wardrobe_folder`]).
//!
//! Move vs copy: `copy` mints a fresh id and `createdAt`/`updatedAt` (`now`);
//! `move` keeps the source id + timestamps and deletes the source after the create.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::collation::locale_compare;
use crate::db::archetype_wardrobe::{
    ensure_project_wardrobe_folder, read_general_wardrobe, read_project_wardrobe,
};
use crate::db::characters_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::ensure_official_store::ensure_official_store;
use crate::db::groups::{GroupEntity, GroupsRepository};
use crate::db::projects::{ProjectEntity, ProjectsRepository};
use crate::db::vault_wardrobe_public::{
    create_project_wardrobe_item, create_vault_wardrobe_item, delete_project_wardrobe_item,
    delete_vault_wardrobe_item, WardrobePublicError,
};
use crate::db::wardrobe_read::find_by_character_id;
use crate::db::DbError;
use crate::vault_overlay::WardrobeItem;

/// The `Wardrobe/` folder (v4 `CHARACTER_WARDROBE_FOLDER`).
const WARDROBE_FOLDER: &str = "Wardrobe";

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

/// The parsed transfer request (v4 `transferRequestSchema`). `destination_id` is
/// required for every scope except `General`.
#[derive(Debug, Clone)]
pub struct TransferRequest {
    pub action: TransferAction,
    pub item_id: String,
    pub source_character_id: String,
    pub source_project_id: Option<String>,
    pub destination_scope: DestinationScope,
    pub destination_id: Option<String>,
}

/// A successful transfer (v4's `{ wardrobeItem, action }`).
pub struct TransferOutcome {
    pub wardrobe_item: Value,
    pub action: TransferAction,
}

/// A transfer failure, mirroring the route's HTTP responses.
#[derive(Debug)]
pub enum TransferError {
    /// v4 `notFound('Wardrobe item')` — the source item didn't resolve.
    NotFound,
    /// v4 `badRequest(msg)` — invalid destination, same source/destination, or an
    /// id collision at the destination.
    BadRequest(String),
    /// v4 `serverError(...)` / an unexpected failure.
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceScope {
    Character,
    Project,
    General,
}

impl SourceScope {
    fn as_str(self) -> &'static str {
        match self {
            SourceScope::Character => "character",
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
/// then the named project, then Quilltap General. Ownership: the source character
/// must belong to `user_id`.
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
        .into_iter()
        .find(|it| item_id_of(it) == Some(item_id))
    {
        return Ok(Some(ResolvedSource {
            scope: SourceScope::Character,
            item,
            character_id: Some(source_character_id.to_string()),
            mount_point_id: None,
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
                    .into_iter()
                    .find(|it| item_id_of(it) == Some(item_id))
                {
                    return Ok(Some(ResolvedSource {
                        scope: SourceScope::Project,
                        item,
                        character_id: None,
                        mount_point_id: Some(ensured.mount_point_id),
                    }));
                }
            }
        }
    }

    // TRY 3 — Quilltap General.
    let general = read_general_wardrobe(main, &docs, true)?;
    if let Some(item) = general
        .into_iter()
        .find(|it| item_id_of(it) == Some(item_id))
    {
        return Ok(Some(ResolvedSource {
            scope: SourceScope::General,
            item,
            character_id: None,
            mount_point_id: None,
        }));
    }

    Ok(None)
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
            links.ensure_folder_path(&ensured.mount_point_id, WARDROBE_FOLDER)?;
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
        DestinationScope::Project | DestinationScope::Group => {
            let mp = dest.mount_point_id.as_deref().unwrap_or_default();
            Ok(read_project_wardrobe(&docs, mp, true)?)
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

/// v4 `deleteFromSource` — remove the item from its origin (move only).
fn delete_from_source(
    main: &Connection,
    mount: &Connection,
    source: &ResolvedSource,
) -> Result<bool, TransferError> {
    let links = DocMountFileLinksRepository::new(mount);
    let docs = DocMountDocumentsRepository::new(mount);
    let id = item_id_of(&source.item).unwrap_or_default();
    match source.scope {
        SourceScope::Project => {
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
    // 1. Resolve the source item.
    let Some(source) = resolve_source_item(
        main,
        mount,
        user_id,
        &req.source_character_id,
        req.source_project_id.as_deref(),
        &req.item_id,
    )?
    else {
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

    // 4. Prepare the moved/copied item.
    let mut next_item = WardrobeItem::from_read_value(&source.item);
    let source_created = next_item.created_at.clone();
    let source_updated = next_item.updated_at.clone();
    match req.action {
        TransferAction::Copy => {
            next_item.id = uuid::Uuid::new_v4().to_string();
            next_item.created_at = now.to_string();
            next_item.updated_at = now.to_string();
        }
        TransferAction::Move => {
            // id/timestamps carry over from the source.
            next_item.created_at = source_created;
            next_item.updated_at = source_updated;
        }
    }
    next_item.character_id = Some(match destination.scope {
        DestinationScope::Character => destination.character_id.clone(),
        _ => None,
    });

    // 5. Reject an id collision at the destination.
    let destination_items = read_destination_items(main, mount, &destination)?;
    if destination_items
        .iter()
        .any(|it| item_id_of(it) == Some(next_item.id.as_str()))
    {
        return Err(TransferError::BadRequest(
            "An item with that ID already exists at the destination".to_string(),
        ));
    }

    // 6. Create at the destination.
    let stored = create_at_destination(main, mount, &destination, &next_item)?;

    // 7. Delete from source (move only).
    if req.action == TransferAction::Move && !delete_from_source(main, mount, &source)? {
        return Err(TransferError::Internal(
            "Failed to remove item from source after move".to_string(),
        ));
    }

    Ok(TransferOutcome {
        wardrobe_item: serde_json::to_value(&stored).unwrap_or(Value::Null),
        action: req.action,
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
