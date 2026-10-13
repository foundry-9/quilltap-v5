//! The shared-archetype wardrobe tiers: v4 `lib/mount-index/shared-wardrobe.ts`
//! and its `general-` / `project-` / `group-wardrobe.ts` façades, plus the
//! `WardrobeRepository.findArchetypes` merge.
//!
//! Wardrobe items live in four tiers: a character's own vault, the **group**
//! stores of every group the character belongs to, the chat project's stores,
//! and the singleton **Quilltap General** store (household archetypes).
//! Precedence mirrors the mount pool's — **character > group > project >
//! general**. This module ports the shared-tier reader + its scoped façades +
//! the merge:
//!
//!   - [`read_shared_wardrobe`] — any mount's `Wardrobe/*.md` (v4
//!     `readSharedWardrobe`; `general-`/`project-`/`group-wardrobe.ts` are thin
//!     façades over it, and so are [`read_general_wardrobe`] /
//!     [`read_project_wardrobe`] / [`read_group_wardrobe`] here);
//!   - [`find_archetypes`] — General merged under the scoped tiers (a later
//!     mount's item wins on id collision, v4's insertion-ordered `Map`);
//!   - [`find_archetype_by_id`] — the single-item lookup the public read trio
//!     falls back to;
//!   - [`find_archetypes_in_mounts_attributed`] — the `?scope=group` read KEPT
//!     GROUPED, each item tagged with the group it hangs in (v4 `cc80dc89d`),
//!     plus the read-time [`WardrobeOrigin`] annotation every wardrobe read
//!     attaches ([`with_origin`]).
//!
//! Every reader calls [`read_character_vault_wardrobe`] with `seed_archetypes =
//! false` (these folders ARE the shared set — re-seeding would recurse) and
//! coerces every item's `characterId` to `null` (a shared item belongs to no
//! character). The archived filter reproduces v4's `!item.archivedAt` truthiness.

use rusqlite::Connection;
use serde_json::Value;

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::instance_settings;
use super::tiered_mount_pool::GroupMounts;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::DbError;
use crate::wardrobe_tiers::SharedWardrobeTiers;
use crate::wearable_pool::is_archived_truthy;

// ===========================================================================
// The read-time origin (v4 `lib/wardrobe/wardrobe-container.ts`, `cc80dc89d`)
// ===========================================================================

/// The display name of the singleton General library, as an origin spells it
/// (v4 `GENERAL_WARDROBE_NAME`). The SAME bytes as the built-in store's own
/// name (`services/builtin_mounts.rs`'s `MountSpec` — a unit pin holds them
/// equal; v4 spells both `'Quilltap General'`).
pub const GENERAL_WARDROBE_NAME: &str = "Quilltap General";

/// Which kind of wardrobe a read found an item in (v4
/// `WardrobeContainerScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WardrobeOriginScope {
    Character,
    General,
    Project,
    Group,
}

/// Which wardrobe a collection read found an item in — v4 `WardrobeOrigin`.
///
/// v4's *why*: a READ-TIME annotation attached by the list endpoints on the way
/// out — never persisted, never exported, never accepted on create/update. A
/// garment has no idea which project it lives in; the read that found it does.
/// Serializes exactly `{ "scope", "id", "name" }` in that order; `id` is `null`
/// for General.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WardrobeOrigin {
    pub scope: WardrobeOriginScope,
    /// The container id; `None` (→ `null`) for General.
    pub id: Option<String>,
    /// The container's display name, resolved server-side.
    pub name: String,
}

impl WardrobeOrigin {
    /// A character / project / group container's origin.
    pub fn new(scope: WardrobeOriginScope, id: &str, name: &str) -> Self {
        WardrobeOrigin {
            scope,
            id: Some(id.to_string()),
            name: name.to_string(),
        }
    }
}

/// The origin every Quilltap General read attaches (v4
/// `GENERAL_WARDROBE_ORIGIN`): `{ scope: 'general', id: null, name: 'Quilltap
/// General' }`.
pub fn general_wardrobe_origin() -> WardrobeOrigin {
    WardrobeOrigin {
        scope: WardrobeOriginScope::General,
        id: None,
        name: GENERAL_WARDROBE_NAME.to_string(),
    }
}

/// A group / project wardrobe's resolved store, with the origin its reads
/// attach — v4's factory ok-arm `{ mountPointId, origin }` (`cc80dc89d`), the
/// origin built from the owner row ALREADY loaded (`ownerOrigin(ownerLabel,
/// row)` — no extra read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerStore {
    pub mount_point_id: String,
    pub origin: WardrobeOrigin,
}

/// v4 `withOrigin(items, origin)` — tag every item with the container it came
/// from: `{ ...item, origin }`, so `origin` lands LAST (the items are
/// `preserve_order` maps; an `insert` appends). A non-object item passes
/// through untouched.
///
/// Only the ROUTES call this (and the attributed group read, which only the
/// `?scope=group` route reaches) — no repository read tags, exactly as in v4
/// (`cc80dc89d` tags nothing below the routes). So the `.qtap` export's
/// `origin` strip (v4 `ndjson-writer.ts`; v5 `services/qtap_export/records.rs`)
/// is INERT against v5's own reads: its items come from
/// `wardrobe_read::find_by_character_id`, which never carries `origin`.
pub fn with_origin(items: Vec<Value>, origin: &WardrobeOrigin) -> Vec<Value> {
    let origin = serde_json::to_value(origin).unwrap_or(Value::Null);
    items
        .into_iter()
        .map(|mut item| {
            if let Value::Object(map) = &mut item {
                map.insert("origin".to_string(), origin.clone());
            }
            item
        })
        .collect()
}

/// The `Wardrobe/` folder, shared by every tier (v4 `CHARACTER_WARDROBE_FOLDER`).
const WARDROBE_FOLDER: &str = "Wardrobe";

/// v4 `readSharedWardrobe(mountPointId, includeArchived)`
/// (`shared-wardrobe.ts:56-60`) — read a shared store's `Wardrobe/` items with
/// `characterId` coerced to `null` and the archived filter applied. v4 calls
/// `readCharacterVaultWardrobe(mp)` with no character — the reader's parse
/// scope becomes the mountPointId (`characterId ?? mountPointId`).
///
/// Since `9ecebf986` the reader seeds nothing anywhere: a shared composite
/// whose components live in ANOTHER tier keeps those refs as UUIDs at parse
/// time (bug 187) and the per-request wearable pool resolves them.
pub fn read_shared_wardrobe(
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    let Some(vault) = read_character_vault_wardrobe(
        docs,
        mount_point_id,
        mount_point_id, // v4: characterId ?? mountPointId
    )?
    else {
        return Ok(Vec::new());
    };
    let items = vault
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(items
        .into_iter()
        .map(|mut it| {
            if let Some(obj) = it.as_object_mut() {
                obj.insert("characterId".to_string(), Value::Null);
            }
            it
        })
        .filter(|it| include_archived || !is_archived_truthy(it))
        .collect())
}

/// v4 `readGeneralWardrobe` — the Quilltap General store's shared archetypes, or
/// `[]` when the General mount hasn't been provisioned.
pub fn read_general_wardrobe(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
        return Ok(Vec::new());
    };
    read_shared_wardrobe(docs, &mount_point_id, include_archived)
}

// LANE-REMOVE(P4.D281): v4 deleted project-wardrobe.ts (readProjectWardrobe) in 9ecebf986
/// The pre-`9ecebf986` project façade over [`read_shared_wardrobe`]; callers
/// move to a `WardrobeLocation`'s `read_items`.
pub fn read_project_wardrobe(
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    read_shared_wardrobe(docs, mount_point_id, include_archived)
}

/// v4 `readGroupWardrobe` — a group document store's shared wardrobe. Anything
/// hanging in a group's `Wardrobe/` folder is wearable by every character who
/// belongs to that group — the household livery, the regimental kit, the coats
/// by the door — without any of them owning it.
///
/// The mounts to read are resolved per *character* (never per chat) by
/// [`super::tiered_mount_pool::resolve_group_mount_point_ids_for_character`]: a
/// character never gains a co-participant's group stores.
// LANE-REMOVE(P4.D281): v4 deleted group-wardrobe.ts (readGroupWardrobe) in 9ecebf986
pub fn read_group_wardrobe(
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    read_shared_wardrobe(docs, mount_point_id, include_archived)
}

/// v4 `ensureGeneralWardrobeFolder` — idempotently create `Quilltap
/// General/Wardrobe/`. Returns `(mountPointId, folderId)`; both `None` when the
/// General mount isn't provisioned, `folderId` `None` on a folder-create failure
/// (write paths tolerate the null).
pub fn ensure_general_wardrobe_folder(
    main: &Connection,
    links: &DocMountFileLinksRepository,
) -> Result<(Option<String>, Option<String>), DbError> {
    let Some(mount_point_id) = instance_settings::get_general_mount_point_id(main)? else {
        return Ok((None, None));
    };
    let folder_id = links.ensure_folder_path(&mount_point_id, WARDROBE_FOLDER)?;
    Ok((Some(mount_point_id), folder_id))
}

/// v4 `ensureSharedWardrobeFolder` — idempotently create `<mount>/Wardrobe/`.
/// `ensureProjectWardrobeFolder` / `ensureGroupWardrobeFolder` are v4's scoped
/// façades over it; v5 has the one function both destinations call.
pub fn ensure_shared_wardrobe_folder(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
) -> Result<Option<String>, DbError> {
    links.ensure_folder_path(mount_point_id, WARDROBE_FOLDER)
}

// LANE-REMOVE(P4.D281): v4 deleted project-wardrobe.ts (ensureProjectWardrobeFolder) in 9ecebf986
/// The pre-`9ecebf986` project façade over [`ensure_shared_wardrobe_folder`];
/// callers move to `resolve_wardrobe_location(…, ensure: true)`.
pub fn ensure_project_wardrobe_folder(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
) -> Result<Option<String>, DbError> {
    ensure_shared_wardrobe_folder(links, mount_point_id)
}

// LANE-REMOVE(P4.D281): v4 deleted group-wardrobe.ts (ensureGroupWardrobeFolder) in 9ecebf986
/// The pre-`9ecebf986` group façade over [`ensure_shared_wardrobe_folder`].
pub fn ensure_group_wardrobe_folder(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
) -> Result<Option<String>, DbError> {
    ensure_shared_wardrobe_folder(links, mount_point_id)
}

/// Strip the read-time `origin` tag the pool / `read_shared_tiers` attach, so
/// a pre-`9ecebf986` caller sees the untagged shape the deleted finders
/// answered.
fn without_origin(mut item: Value) -> Value {
    if let Some(obj) = item.as_object_mut() {
        obj.remove("origin");
    }
    item
}

/// The pool-level project origin (`pool.ts:112-116`): a POOL never knows which
/// project a store belongs to, so `{ scope: 'project', id: null, name:
/// 'Project' }`.
pub fn pool_project_origin() -> WardrobeOrigin {
    WardrobeOrigin {
        scope: WardrobeOriginScope::Project,
        id: None,
        name: "Project".to_string(),
    }
}

// LANE-REMOVE(P4.D281): v4 deleted WardrobeRepository.findArchetypesInMountsAttributed in 9ecebf986
/// The pre-`9ecebf986` attributed group read — now
/// [`super::wardrobe_read::read_shared_tiers`] over the groups' mounts with
/// each group's origin (what v4's `?scope=group` route does at the pin through
/// `groupLocationsForCharacter`). The tags STAY (that is what the caller
/// wanted).
pub fn find_archetypes_in_mounts_attributed(
    docs: &DocMountDocumentsRepository,
    groups: &[GroupMounts],
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let mut origin_by_mount: std::collections::HashMap<String, WardrobeOrigin> =
        std::collections::HashMap::new();
    let mut mounts: Vec<String> = Vec::new();
    for g in groups {
        for mp in &g.mount_point_ids {
            origin_by_mount.insert(
                mp.clone(),
                WardrobeOrigin::new(WardrobeOriginScope::Group, &g.group.id, &g.group.name),
            );
            mounts.push(mp.clone());
        }
    }
    Ok(super::wardrobe_read::read_shared_tiers(
        docs,
        &mounts,
        include_archived,
        &|mp| {
            origin_by_mount
                .get(mp)
                .cloned()
                .unwrap_or_else(pool_project_origin)
        },
    ))
}

// LANE-REMOVE(P4.D282|P4.D281|P4.D280): v4 deleted WardrobeRepository.findArchetypes in 9ecebf986
/// The pre-`9ecebf986` "General merged under the scoped tiers" read, untagged:
/// General, then the project stores, then the group stores (weakest first, a
/// later mount shadowing in place) — exactly the shared layers of a
/// [`crate::wearable_pool::WearablePool`]. Callers move to the pool.
pub fn find_archetypes(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    include_archived: bool,
    tiers: &SharedWardrobeTiers,
) -> Result<Vec<Value>, DbError> {
    let mut mounts: Vec<String> = Vec::new();
    if let Some(general) = instance_settings::get_general_mount_point_id(main)? {
        mounts.push(general);
    }
    let general_count = mounts.len();
    mounts.extend(tiers.scoped_mounts());
    if mounts.is_empty() {
        return Ok(Vec::new());
    }
    let general_id = mounts.first().filter(|_| general_count == 1).cloned();
    Ok(
        super::wardrobe_read::read_shared_tiers(docs, &mounts, include_archived, &|mp| {
            if general_id.as_deref() == Some(mp) {
                general_wardrobe_origin()
            } else {
                pool_project_origin()
            }
        })
        .into_iter()
        .map(without_origin)
        .collect(),
    )
}

// LANE-REMOVE(P4.D281): v4 deleted WardrobeRepository.findArchetypeById in 9ecebf986
/// The pre-`9ecebf986` single shared item by id (archived included); callers
/// move to `pool.get(id)`.
pub fn find_archetype_by_id(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    id: &str,
    tiers: &SharedWardrobeTiers,
) -> Result<Option<Value>, DbError> {
    let archetypes = find_archetypes(main, docs, true, tiers)?;
    Ok(archetypes
        .into_iter()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn origin_shapes_and_the_general_name_matches_the_builtin_store() {
        assert_eq!(
            serde_json::to_value(general_wardrobe_origin()).unwrap(),
            json!({ "scope": "general", "id": null, "name": "Quilltap General" })
        );
        assert_eq!(
            serde_json::to_string(&WardrobeOrigin::new(
                WardrobeOriginScope::Character,
                "c1",
                "Abigail"
            ))
            .unwrap(),
            r#"{"scope":"character","id":"c1","name":"Abigail"}"#
        );
        // Sourced from the built-in store's name, never re-spelled.
        let builtin = include_str!("../services/builtin_mounts.rs");
        assert!(builtin.contains(&format!("name: \"{GENERAL_WARDROBE_NAME}\",")));
        // An item that already carries `origin` is re-tagged in place.
        let out = with_origin(
            vec![json!({"id": "a", "origin": 1, "z": 2})],
            &general_wardrobe_origin(),
        );
        assert_eq!(
            out[0].as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["id", "origin", "z"]
        );
    }
}
