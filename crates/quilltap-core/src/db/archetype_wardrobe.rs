//! The shared-archetype wardrobe tiers (v4 `lib/mount-index/general-wardrobe.ts`
//! + `project-wardrobe.ts` + the `WardrobeRepository.findArchetypes` merge).
//!
//! Wardrobe items live in three tiers: a character's own vault, the singleton
//! **Quilltap General** store (household archetypes), and optional **project**
//! stores that shadow General within a project. This module ports the two
//! shared-tier readers + the merge:
//!
//!   - [`read_general_wardrobe`] — the General store's `Wardrobe/*.md`, or `[]`
//!     when the General mount isn't provisioned;
//!   - [`read_project_wardrobe`] — a specific project store's `Wardrobe/*.md`;
//!   - [`find_archetypes`] — General merged under any project tiers (project
//!     items win on id collision, v4's insertion-ordered `Map`);
//!   - [`find_archetype_by_id`] — the single-item lookup the public read trio
//!     falls back to.
//!
//! Both readers call [`read_character_vault_wardrobe`] with `seed_archetypes =
//! false` (these folders ARE the shared set — re-seeding would recurse) and
//! coerce every item's `characterId` to `null` (a shared item belongs to no
//! character). The archived filter reproduces v4's `!item.archivedAt` truthiness.

use rusqlite::Connection;
use serde_json::Value;

use super::doc_mount_documents::DocMountDocumentsRepository;
use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::instance_settings;
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::DbError;
use crate::wearable_pool::is_archived_truthy;

/// The `Wardrobe/` folder, shared by every tier (v4 `CHARACTER_WARDROBE_FOLDER`).
const WARDROBE_FOLDER: &str = "Wardrobe";

/// Read a shared store's `Wardrobe/` items with `characterId` coerced to `null`
/// and the archived filter applied. v4 calls `readCharacterVaultWardrobe(mount,
/// undefined, { seedArchetypes: false })` — with `characterId` undefined the
/// reader's parse scope becomes the mountPointId (v4 `characterId ?? mountPointId`).
fn read_shared_wardrobe(
    docs: &DocMountDocumentsRepository,
    mount_point_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    let Some(vault) = read_character_vault_wardrobe(
        docs,
        mount_point_id,
        mount_point_id, // v4: characterId ?? mountPointId
        false,          // shared folders never seed archetypes
        &|| Ok(Vec::new()),
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

/// v4 `readProjectWardrobe` — a specific project store's shared archetypes.
///
/// Archetype seeding is disabled in the underlying reader: a project composite
/// resolves its components within this same folder, not by recursing through
/// [`find_archetypes`] (which would loop back here).
///
/// Consequence, and a known gap: a project composite whose components live in
/// *Quilltap General* loses those refs at parse time — the overlay's
/// component check only sees this folder's items. Same-tier composites (the
/// common case) are fine. Read-time hydration in
/// [`crate::tools::wardrobe_shared::resolve_equipped_outfit_leaf_values`]
/// recovers the equipped case; the parse-time gap is tracked separately.
pub fn read_project_wardrobe(
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

/// v4 `ensureProjectWardrobeFolder` — idempotently create `<projectMount>/Wardrobe/`.
pub fn ensure_project_wardrobe_folder(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
) -> Result<Option<String>, DbError> {
    links.ensure_folder_path(mount_point_id, WARDROBE_FOLDER)
}

/// v4 `WardrobeRepository.findArchetypes` — the General tier merged under any
/// project tiers. A project item shadows a General item on id collision; the
/// output order is v4's insertion-ordered `Map`: General items in order, then
/// project-only items appended in project-mount order (a shadowing project item
/// replaces the value at the General item's position). A project read that fails
/// is skipped (v4 logs a warning and continues).
pub fn find_archetypes(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    include_archived: bool,
    project_mount_point_ids: &[String],
) -> Result<Vec<Value>, DbError> {
    let general = read_general_wardrobe(main, docs, include_archived)?;
    if project_mount_point_ids.is_empty() {
        return Ok(general);
    }

    // Insertion-ordered merge (v4's `Map`): a Vec for order + id→index for
    // shadowing. `set(id, item)` on an existing id replaces in place; a new id
    // appends.
    let mut order: Vec<Value> = Vec::new();
    let mut index_by_id: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut upsert = |item: Value| {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(&i) = index_by_id.get(&id) {
            order[i] = item;
        } else {
            index_by_id.insert(id, order.len());
            order.push(item);
        }
    };

    for item in general {
        upsert(item);
    }
    for mount_point_id in project_mount_point_ids {
        // v4 wraps each project read in try/catch and skips on failure.
        if let Ok(project_items) = read_project_wardrobe(docs, mount_point_id, include_archived) {
            for item in project_items {
                upsert(item);
            }
        }
    }
    Ok(order)
}

/// v4 `WardrobeRepository.findArchetypeById` — the single shared item by id, or
/// `None` if no tier holds it. Always reads with `includeArchived = true` (equip
/// paths need an archived item's `types`).
pub fn find_archetype_by_id(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    id: &str,
    project_mount_point_ids: &[String],
) -> Result<Option<Value>, DbError> {
    let archetypes = find_archetypes(main, docs, true, project_mount_point_ids)?;
    Ok(archetypes
        .into_iter()
        .find(|a| a.get("id").and_then(Value::as_str) == Some(id)))
}
