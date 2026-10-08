//! The wardrobe CONTAINER resolver and an item's HOME in it — v4
//! `lib/wardrobe/resolve-container.ts` (`resolveWardrobeContainer`,
//! `0506517d3`) and the first section of `lib/wardrobe/item-images.ts`
//! (`7c8572869`, #82: `resolveContainerMountPointId`,
//! `resolveWardrobeItemHome`).
//!
//! P4.D263 R-E: v5 had no `resolveWardrobeContainer` port (the transfers
//! service re-derives containers inline — `wardrobe_transfers.rs`'
//! `resolve_explicit_source` / `resolve_destination`, which stay as they are),
//! and the item-images route, the generation and the job handler all need v4's
//! `WardrobeItemHome`. This is the ONE shared resolver they read.
//!
//! v4's container resolve ENSURES a project's or group's official store and
//! its `Wardrobe/` folder on the way ("every caller is about to read or write
//! there") — so even a list WRITES for those two scopes; callers run it on the
//! writer.
//!
//! The home's `resolve_mount` is the tombstone gate: for an ARCHIVED
//! character's item it answers [`DbError::CharacterArchived`] (v4
//! `resolveWardrobeMount` throws `CharacterArchivedError`) — callers let it
//! propagate, never fall back.

use rusqlite::Connection;
use serde_json::Value;

use crate::api::types::WardrobeContainerScope;
use crate::db::archetype_wardrobe::{
    ensure_group_wardrobe_folder, ensure_project_wardrobe_folder, read_general_wardrobe,
    read_group_wardrobe, read_project_wardrobe,
};
use crate::db::characters_read;
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::ensure_official_store::ensure_official_store;
use crate::db::groups::{GroupEntity, GroupsRepository};
use crate::db::instance_settings;
use crate::db::projects::{ProjectEntity, ProjectsRepository};
use crate::db::vault_wardrobe_public::{
    resolve_character_wardrobe_mount_point, update_project_wardrobe_item,
    update_vault_wardrobe_item, WardrobePatch, WardrobePublicError,
};
use crate::db::wardrobe_read::find_by_character_id;
use crate::db::DbError;
use crate::vault_overlay::WardrobeItem;

/// v4 `WardrobeContainerScope` on the wire (`'character' | 'general' |
/// 'project' | 'group'`).
pub fn scope_str(scope: WardrobeContainerScope) -> &'static str {
    match scope {
        WardrobeContainerScope::Character => "character",
        WardrobeContainerScope::General => "general",
        WardrobeContainerScope::Project => "project",
        WardrobeContainerScope::Group => "group",
    }
}

/// v4 `ResolvedWardrobeContainer` with its `readItems()` already read (every
/// caller reads them at once).
#[derive(Debug, Clone)]
pub struct ResolvedWardrobeContainer {
    pub scope: WardrobeContainerScope,
    /// Owning character — `character` scope only.
    pub character_id: Option<String>,
    /// Backing mount — `project` / `group` scopes only.
    pub mount_point_id: Option<String>,
    /// Every item in the container, archived included.
    pub items: Vec<Value>,
}

/// v4 `resolveWardrobeContainer(scope, id, repos, userId)` — `None` when the
/// owning entity is missing (or, for a character, not `user_id`'s), when a
/// non-General scope arrives without an id, or when a project/group store
/// cannot be ensured. Logs v4's DEBUG `[WardrobeContainer] Resolved wardrobe
/// container` either way.
pub fn resolve_wardrobe_container(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: WardrobeContainerScope,
    id: Option<&str>,
) -> Result<Option<ResolvedWardrobeContainer>, DbError> {
    let resolved = resolve_container(main, mount, user_id, scope, id)?;
    tracing::debug!(
        scope = scope_str(scope),
        id = id.unwrap_or("null"),
        found = resolved.is_some(),
        mountPointId = resolved
            .as_ref()
            .and_then(|r| r.mount_point_id.as_deref())
            .unwrap_or("null"),
        context = "wardrobe",
        "[WardrobeContainer] Resolved wardrobe container"
    );
    Ok(resolved)
}

fn resolve_container(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: WardrobeContainerScope,
    id: Option<&str>,
) -> Result<Option<ResolvedWardrobeContainer>, DbError> {
    let docs = DocMountDocumentsRepository::new(mount);
    if scope == WardrobeContainerScope::General {
        return Ok(Some(ResolvedWardrobeContainer {
            scope,
            character_id: None,
            mount_point_id: None,
            items: read_general_wardrobe(main, &docs, true)?,
        }));
    }

    // v4 `if (!id) return null` — an empty string is falsy too.
    let Some(id) = id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };

    if scope == WardrobeContainerScope::Character {
        let Some(character) = characters_read::find_by_id_raw(main, id)? else {
            return Ok(None);
        };
        if character.get("userId").and_then(Value::as_str) != Some(user_id) {
            return Ok(None);
        }
        let character_id = character
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_string();
        let items = find_by_character_id(main, &docs, &character_id, true)?;
        return Ok(Some(ResolvedWardrobeContainer {
            scope,
            character_id: Some(character_id),
            mount_point_id: None,
            items,
        }));
    }

    let links = DocMountFileLinksRepository::new(mount);
    let ensured = if scope == WardrobeContainerScope::Project {
        let Some(project) = ProjectsRepository::new(main, mount)
            .find_by_id(id)
            .map_err(|e| e.into_db())?
        else {
            return Ok(None);
        };
        let name = entity_name(&project, "Project");
        let Some(ensured) = ensure_official_store::<ProjectEntity>(main, mount, id, &name)? else {
            return Ok(None);
        };
        ensure_project_wardrobe_folder(&links, &ensured.mount_point_id)?;
        ensured
    } else {
        let Some(group) = GroupsRepository::new(main, mount)
            .find_by_id(id)
            .map_err(|e| e.into_db())?
        else {
            return Ok(None);
        };
        let name = entity_name(&group, "Group");
        let Some(ensured) = ensure_official_store::<GroupEntity>(main, mount, id, &name)? else {
            return Ok(None);
        };
        ensure_group_wardrobe_folder(&links, &ensured.mount_point_id)?;
        ensured
    };
    let items = if scope == WardrobeContainerScope::Project {
        read_project_wardrobe(&docs, &ensured.mount_point_id, true)?
    } else {
        read_group_wardrobe(&docs, &ensured.mount_point_id, true)?
    };
    Ok(Some(ResolvedWardrobeContainer {
        scope,
        character_id: None,
        mount_point_id: Some(ensured.mount_point_id),
        items,
    }))
}

/// v4 `entity.name || '<Fallback>'`.
fn entity_name(entity: &Value, fallback: &str) -> String {
    entity
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

/// v4 `resolveContainerMountPointId(scope, characterId, mountPointId)` — the
/// mount a container's items (and their pictures) live in. Throws v4's three
/// sentences (as [`DbError::Internal`]); for an archived character
/// [`DbError::CharacterArchived`] — that is the point.
pub fn resolve_container_mount_point_id(
    main: &Connection,
    scope: WardrobeContainerScope,
    character_id: Option<&str>,
    mount_point_id: Option<&str>,
) -> Result<String, DbError> {
    match scope {
        WardrobeContainerScope::Project | WardrobeContainerScope::Group => mount_point_id
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                DbError::Internal(format!(
                    "No store mount resolved for {} wardrobe",
                    scope_str(scope)
                ))
            }),
        WardrobeContainerScope::General => instance_settings::get_general_mount_point_id(main)?
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DbError::Internal("Quilltap General is not provisioned".to_string())),
        WardrobeContainerScope::Character => {
            let cid = character_id.unwrap_or_default();
            resolve_character_wardrobe_mount_point(main, cid)?.ok_or_else(|| {
                // v4 interpolates the id as given (`null` renders "null").
                DbError::Internal(format!(
                    "Character {} has no linked vault",
                    character_id.unwrap_or("null")
                ))
            })
        }
    }
}

/// v4 `WardrobeItemHome` — an item located in its container, with the means
/// to write it back.
#[derive(Debug, Clone)]
pub struct WardrobeItemHome {
    pub scope: WardrobeContainerScope,
    /// The owning character — `character` scope only.
    pub character_id: Option<String>,
    /// The container's backing mount — `project` / `group` scopes only.
    pub container_mount_point_id: Option<String>,
    /// The item as read (`WardrobeItemFromFile`-shaped JSON).
    pub item: Value,
    /// Every item in the container (archived included).
    pub container_items: Vec<Value>,
}

impl WardrobeItemHome {
    pub fn item_id(&self) -> &str {
        self.item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    pub fn item_title(&self) -> &str {
        self.item
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
    }

    /// The item's current `imageFileId` (v4 `item.imageFileId ?? null` — the
    /// parser already reads an EMPTY string as `null`).
    pub fn image_file_id(&self) -> Option<&str> {
        self.item.get("imageFileId").and_then(Value::as_str)
    }

    /// v4 `home.resolveMount()` — the mount holding the item's markdown (where
    /// its pictures go). [`DbError::CharacterArchived`] for a tombstone.
    pub fn resolve_mount(&self, main: &Connection) -> Result<String, DbError> {
        resolve_container_mount_point_id(
            main,
            self.scope,
            self.character_id.as_deref(),
            self.container_mount_point_id.as_deref(),
        )
    }

    /// v4 `home.update(patch)` — the tier's ordinary update chokepoint:
    /// project/group → `updateProjectWardrobeItem(mountPointId, …)`; else
    /// `repos.wardrobe.update(itemId, patch, scope === 'character' ?
    /// characterId : null)`.
    pub fn update(
        &self,
        main: &Connection,
        mount: &Connection,
        patch: &WardrobePatch,
    ) -> Result<Option<WardrobeItem>, WardrobePublicError> {
        let links = DocMountFileLinksRepository::new(mount);
        let docs = DocMountDocumentsRepository::new(mount);
        match self.scope {
            WardrobeContainerScope::Project | WardrobeContainerScope::Group => {
                update_project_wardrobe_item(
                    main,
                    &links,
                    &docs,
                    self.container_mount_point_id.as_deref().unwrap_or_default(),
                    self.item_id(),
                    patch,
                )
            }
            WardrobeContainerScope::Character => update_vault_wardrobe_item(
                main,
                &links,
                &docs,
                self.item_id(),
                patch,
                self.character_id.as_deref(),
            ),
            WardrobeContainerScope::General => {
                update_vault_wardrobe_item(main, &links, &docs, self.item_id(), patch, None)
            }
        }
    }
}

/// v4 `resolveWardrobeItemHome(repos, userId, scope, containerId, itemId)` —
/// find `item_id` in the named container, or `None` when the container does
/// not resolve or does not hold the item. A General archetype merged into a
/// character's read is NOT in the character's own wardrobe (the
/// `i.characterId === container.characterId` test).
pub fn resolve_wardrobe_item_home(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Result<Option<WardrobeItemHome>, DbError> {
    let Some(container) = resolve_wardrobe_container(main, mount, user_id, scope, container_id)?
    else {
        return Ok(None);
    };
    let item = container
        .items
        .iter()
        .find(|i| {
            i.get("id").and_then(Value::as_str) == Some(item_id)
                && (scope != WardrobeContainerScope::Character
                    || i.get("characterId").and_then(Value::as_str)
                        == container.character_id.as_deref())
        })
        .cloned();
    let Some(item) = item else {
        return Ok(None);
    };
    Ok(Some(WardrobeItemHome {
        scope,
        character_id: container.character_id,
        container_mount_point_id: container.mount_point_id,
        item,
        container_items: container.items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_main() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let schema: Value =
            serde_json::from_str(include_str!("provisioning/fresh_schema.json")).unwrap();
        for sql in schema["main"].as_array().unwrap() {
            let sql = sql.as_str().unwrap();
            if sql.starts_with("CREATE TABLE") {
                conn.execute_batch(sql).unwrap();
            }
        }
        conn
    }

    fn message(r: Result<String, DbError>) -> String {
        match r {
            Err(DbError::Internal(m)) => m,
            other => panic!("expected v4's thrown sentence, got {other:?}"),
        }
    }

    /// v4 `resolveContainerMountPointId`'s three sentences (`item-images.ts:
    /// 88, 93, 97`), byte-exact.
    #[test]
    fn the_three_container_mount_sentences() {
        let main = fresh_main();
        for (scope, word) in [
            (WardrobeContainerScope::Project, "project"),
            (WardrobeContainerScope::Group, "group"),
        ] {
            assert_eq!(
                message(resolve_container_mount_point_id(&main, scope, None, None)),
                format!("No store mount resolved for {word} wardrobe")
            );
            assert_eq!(
                resolve_container_mount_point_id(&main, scope, None, Some("mp-1")).unwrap(),
                "mp-1"
            );
        }
        assert_eq!(
            message(resolve_container_mount_point_id(
                &main,
                WardrobeContainerScope::General,
                None,
                None
            )),
            "Quilltap General is not provisioned"
        );
        assert_eq!(
            message(resolve_container_mount_point_id(
                &main,
                WardrobeContainerScope::Character,
                Some("c-404"),
                None
            )),
            "Character c-404 has no linked vault"
        );
        main.execute(
            "INSERT INTO instance_settings (key, value) VALUES ('generalMountPointId', 'gen-mp')",
            [],
        )
        .unwrap();
        assert_eq!(
            resolve_container_mount_point_id(&main, WardrobeContainerScope::General, None, None)
                .unwrap(),
            "gen-mp"
        );
    }

    #[test]
    fn scope_wire_strings() {
        for s in ["character", "general", "project", "group"] {
            let scope: WardrobeContainerScope =
                serde_json::from_value(serde_json::json!(s)).unwrap();
            assert_eq!(scope_str(scope), s);
        }
    }
}
