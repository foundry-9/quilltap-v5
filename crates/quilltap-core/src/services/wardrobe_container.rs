//! An item's HOME in its tier — the first section of v4
//! `lib/wardrobe/item-images.ts` (`resolveWardrobeItemHome`, `WardrobeItemHome`)
//! at `4a38222e6`, built FROM the wardrobe location
//! ([`crate::db::wardrobe_location::resolve_wardrobe_location`]).
//!
//! **`9ecebf986` deleted `lib/wardrobe/resolve-container.ts`.** The container
//! resolver v5 ported as `resolve_wardrobe_container` (P4.D263 R-E) is
//! replaced by the location: the home is `resolveWardrobeLocation(scope,
//! containerId, repos, userId)` — a READ, so a project or group store that
//! doesn't exist yet is NOT provisioned (R-C; the old resolver ensured a store
//! on every resolve, and v5 recorded that write-on-read as deliberate — that
//! record is retired here) — then `location.readItems(true)` and the item by
//! id. `resolveMount` is `location.writableMountPointId()` (the tombstone
//! gate: an ARCHIVED character's item answers [`DbError::CharacterArchived`] —
//! callers let it propagate, never fall back), and `update` is
//! `location.update(itemId, patch)`.
//!
//! The two pre-`9ecebf986` resolvers (`resolve_wardrobe_container`,
//! `resolve_container_mount_point_id`) are gone: their last caller, the
//! transfers service, moved onto the location (P4.D281).

use rusqlite::Connection;
use serde_json::Value;

use crate::api::types::WardrobeContainerScope;
use crate::db::vault_wardrobe_public::{WardrobePatch, WardrobePublicError, WardrobeScope};
use crate::db::wardrobe_location::{
    resolve_wardrobe_location, ResolveWardrobeLocationOptions, WardrobeLocation,
    WardrobeLocationError,
};
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

/// The wire scope as the location names it (the same four words).
pub fn wardrobe_scope(scope: WardrobeContainerScope) -> WardrobeScope {
    match scope {
        WardrobeContainerScope::Character => WardrobeScope::Character,
        WardrobeContainerScope::General => WardrobeScope::General,
        WardrobeContainerScope::Project => WardrobeScope::Project,
        WardrobeContainerScope::Group => WardrobeScope::Group,
    }
}

/// v4 `WardrobeItemHome` — an item located in its tier, with the means to
/// write it back.
#[derive(Debug, Clone)]
pub struct WardrobeItemHome {
    pub scope: WardrobeContainerScope,
    /// The owning character — `character` scope only.
    pub character_id: Option<String>,
    /// The container's backing mount — `project` / `group` scopes only (the
    /// pre-location shape some readers still consult; the location carries the
    /// mount for every tier).
    pub container_mount_point_id: Option<String>,
    /// The item as read (`WardrobeItemFromFile`-shaped JSON).
    pub item: Value,
    /// Every item in the item's folder (archived included).
    pub container_items: Vec<Value>,
    /// The item's tier.
    pub location: WardrobeLocation,
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
    /// its pictures go): `location.writableMountPointId()`.
    /// [`DbError::CharacterArchived`] for a tombstone; a vanished vault is v4's
    /// thrown `Character <id> has no linked vault`.
    pub fn resolve_mount(&self, main: &Connection) -> Result<String, DbError> {
        self.location
            .writable_mount_point_id(main)
            .map_err(location_error_to_db)
    }

    /// v4 `home.update(patch)` — `location.update(itemId, patch)`.
    pub fn update(
        &self,
        main: &Connection,
        mount: &Connection,
        patch: &WardrobePatch,
    ) -> Result<Option<WardrobeItem>, WardrobePublicError> {
        Ok(self.location.update(main, mount, self.item_id(), patch)?)
    }
}

/// A location error as the pre-location callers see it (a plain `DbError`).
fn location_error_to_db(e: WardrobeLocationError) -> DbError {
    match e {
        WardrobeLocationError::CharacterArchived { character_id } => {
            DbError::CharacterArchived { character_id }
        }
        WardrobeLocationError::Db(d) => d,
        other => DbError::Internal(other.message()),
    }
}

/// v4 `resolveWardrobeItemHome(repos, userId, scope, containerId, itemId)` —
/// find `item_id` in the named tier, or `None` when the tier does not resolve
/// or does not hold the item (a General item is not in a character's own
/// wardrobe, even though the character's reads merge it in — the location
/// reads the vault FOLDER alone). A read: a project or group store that
/// doesn't exist yet is not provisioned.
pub fn resolve_wardrobe_item_home(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    scope: WardrobeContainerScope,
    container_id: Option<&str>,
    item_id: &str,
) -> Result<Option<WardrobeItemHome>, DbError> {
    let Some(location) = resolve_wardrobe_location(
        main,
        mount,
        wardrobe_scope(scope),
        container_id,
        user_id,
        ResolveWardrobeLocationOptions { ensure: false },
    )?
    else {
        return Ok(None);
    };
    let container_items: Vec<Value> = location
        .read_items(mount, true)?
        .iter()
        .map(|item| serde_json::to_value(item).expect("serialize wardrobe item"))
        .collect();
    let Some(item) = container_items
        .iter()
        .find(|i| i.get("id").and_then(Value::as_str) == Some(item_id))
        .cloned()
    else {
        return Ok(None);
    };
    Ok(Some(WardrobeItemHome {
        scope,
        character_id: location.character_id.clone(),
        container_mount_point_id: match scope {
            WardrobeContainerScope::Project | WardrobeContainerScope::Group => {
                Some(location.mount_point_id.clone())
            }
            _ => None,
        },
        item,
        container_items,
        location,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_wire_strings_and_the_location_scope_agree() {
        for s in ["character", "general", "project", "group"] {
            let scope: WardrobeContainerScope =
                serde_json::from_value(serde_json::json!(s)).unwrap();
            assert_eq!(scope_str(scope), s);
            assert_eq!(wardrobe_scope(scope).as_str(), s);
        }
    }
}
