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
//!   - [`find_archetypes_in_mounts`] — the scoped read on its own, which the
//!     `?scope=group` route and the chat-start pool's per-character group tier
//!     both reach for directly;
//!   - [`find_archetype_by_id`] — the single-item lookup the public read trio
//!     falls back to.
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
use super::vault_read_overlay::read_character_vault_wardrobe;
use super::DbError;
use crate::wardrobe_tiers::SharedWardrobeTiers;
use crate::wearable_pool::is_archived_truthy;

/// The `Wardrobe/` folder, shared by every tier (v4 `CHARACTER_WARDROBE_FOLDER`).
const WARDROBE_FOLDER: &str = "Wardrobe";

/// v4 `readSharedWardrobe` — read a shared store's `Wardrobe/` items with
/// `characterId` coerced to `null` and the archived filter applied. v4 calls
/// `readCharacterVaultWardrobe(mount, undefined, { seedArchetypes: false })` —
/// with `characterId` undefined the reader's parse scope becomes the
/// mountPointId (v4 `characterId ?? mountPointId`).
///
/// Archetype seeding is disabled in the underlying reader: a shared composite
/// resolves its components within this same folder, not by recursing through
/// [`find_archetypes`] (which would loop back here).
///
/// Consequence, and a known gap: a shared composite whose components live in a
/// *different* tier loses those refs at parse time — the overlay's component
/// check only sees this folder's items. Same-tier composites (the common case)
/// are fine. Read-time hydration in
/// [`crate::tools::wardrobe_shared::resolve_equipped_outfit_leaf_values`]
/// recovers the equipped case; the parse-time gap is tracked separately.
pub fn read_shared_wardrobe(
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

/// v4 `readProjectWardrobe` — a project store's shared archetypes. A
/// project-scoped façade over [`read_shared_wardrobe`]; see it for the
/// parse-time composite caveat.
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

/// v4 `ensureProjectWardrobeFolder` — idempotently create `<projectMount>/Wardrobe/`.
pub fn ensure_project_wardrobe_folder(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
) -> Result<Option<String>, DbError> {
    ensure_shared_wardrobe_folder(links, mount_point_id)
}

/// v4 `ensureGroupWardrobeFolder` — idempotently create `<groupMount>/Wardrobe/`.
pub fn ensure_group_wardrobe_folder(
    links: &DocMountFileLinksRepository,
    mount_point_id: &str,
) -> Result<Option<String>, DbError> {
    ensure_shared_wardrobe_folder(links, mount_point_id)
}

/// An insertion-ordered `id → item` upsert accumulator — v4's `Map`, where
/// `set(id, item)` on an existing id replaces the value **in place** (keeping
/// its position) and a new id appends.
struct OrderedById {
    order: Vec<Value>,
    index_by_id: std::collections::HashMap<String, usize>,
}

impl OrderedById {
    fn new() -> Self {
        OrderedById {
            order: Vec::new(),
            index_by_id: std::collections::HashMap::new(),
        }
    }

    fn upsert(&mut self, item: Value) {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(&i) = self.index_by_id.get(&id) {
            self.order[i] = item;
        } else {
            self.index_by_id.insert(id, self.order.len());
            self.order.push(item);
        }
    }
}

/// v4 `WardrobeRepository.findArchetypesInMounts` — read the shared `Wardrobe/`
/// folder of each given mount, in order. A later mount's item shadows an earlier
/// one with the same id, so callers pass their mounts **weakest tier first**
/// (see [`SharedWardrobeTiers::scoped_mounts`]).
///
/// A mount that can't be read is logged and skipped: one unreadable group store
/// must not cost a character the rest of their wardrobe.
pub fn find_archetypes_in_mounts(
    docs: &DocMountDocumentsRepository,
    mount_point_ids: &[String],
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    Ok(merge_mounts(mount_point_ids, |mount_point_id| {
        read_shared_wardrobe(docs, mount_point_id, include_archived)
    }))
}

/// The merge rule [`find_archetypes_in_mounts`] applies, over an injected
/// reader — the seam v4's own unit suite gets by mocking `readSharedWardrobe`.
/// Later mounts shadow earlier ones; a mount whose read fails is logged and
/// skipped, so one unreadable group store cannot cost a character the rest of
/// their wardrobe.
fn merge_mounts<F>(mount_point_ids: &[String], mut read: F) -> Vec<Value>
where
    F: FnMut(&str) -> Result<Vec<Value>, DbError>,
{
    if mount_point_ids.is_empty() {
        return Vec::new();
    }
    let mut acc = OrderedById::new();
    for mount_point_id in mount_point_ids {
        match read(mount_point_id) {
            Ok(items) => {
                for item in items {
                    acc.upsert(item);
                }
            }
            Err(e) => {
                tracing::warn!(
                    mount_point_id, context = "wardrobe", error = %e,
                    "Failed to read shared wardrobe tier; skipping"
                );
            }
        }
    }
    acc.order
}

/// v4 `WardrobeRepository.findArchetypes` — the General tier merged under the
/// scoped tiers (`tiers.scoped_mounts()`, project-then-group so group wins).
///
/// Precedence follows the mount pool's — **character > group > project >
/// general** — so a group's own livery shadows a project's version of the same
/// item, and both shadow the household archetype. The character tier is handled
/// by callers via `find_by_character_id`.
///
/// The output order is v4's insertion-ordered `Map`: General items in order,
/// then scoped-only items appended in mount order (a shadowing scoped item
/// replaces the value at the General item's position).
pub fn find_archetypes(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    include_archived: bool,
    tiers: &SharedWardrobeTiers,
) -> Result<Vec<Value>, DbError> {
    let general = read_general_wardrobe(main, docs, include_archived)?;

    // Weakest tier first: later writes win on id collision.
    let scoped = tiers.scoped_mounts();
    if scoped.is_empty() {
        return Ok(general);
    }

    let mut acc = OrderedById::new();
    for item in general {
        acc.upsert(item);
    }
    for item in find_archetypes_in_mounts(docs, &scoped, include_archived)? {
        acc.upsert(item);
    }
    Ok(acc.order)
}

/// v4 `WardrobeRepository.findArchetypeById` — the single shared item by id, or
/// `None` if no tier holds it. Always reads with `includeArchived = true` (equip
/// paths need an archived item's `types`).
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

    /// v4 `__tests__/unit/lib/database/repositories/wardrobe.repository.pool.test.ts`
    /// mocks `readSharedWardrobe` and asserts the merge rule alone. v5's
    /// equivalent seam is [`merge_mounts`]'s injected reader; the arms below are
    /// that suite's, case for case, for the two claims the DB differentials
    /// cannot reach — a failing store, and a same-tier collision. (The
    /// end-to-end precedence claim — character > group > project > general — is
    /// proven live instead, by the `wardrobe_tools` and `outfit_llm_choose`
    /// families against v4's real code.)
    fn item(id: &str, tier: &str) -> Value {
        json!({ "id": id, "characterId": null, "title": format!("{id} ({tier})") })
    }

    fn titles(items: &[Value]) -> Vec<String> {
        items
            .iter()
            .map(|i| i["title"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_last_mount_wins_a_collision_between_stores_of_the_same_tier() {
        let out = merge_mounts(&ids(&["mp-1", "mp-2"]), |mp| {
            Ok(vec![item("livery", &format!("project-{mp}"))])
        });
        assert_eq!(titles(&out), vec!["livery (project-mp-2)"]);
    }

    #[test]
    fn a_failing_store_is_skipped_and_the_others_survive() {
        let out = merge_mounts(&ids(&["grp-broken", "mp-ok"]), |mp| {
            if mp == "grp-broken" {
                return Err(DbError::Internal("store offline".into()));
            }
            Ok(vec![item("p-only", "project")])
        });
        assert_eq!(titles(&out), vec!["p-only (project)"]);
    }

    #[test]
    fn no_scoped_mounts_short_circuits_before_touching_the_reader() {
        let mut calls = 0usize;
        let out = merge_mounts(&[], |_| {
            calls += 1;
            Ok(Vec::new())
        });
        assert!(out.is_empty());
        assert_eq!(calls, 0, "the reader must not be touched at all");
    }

    #[test]
    fn a_shadowing_item_keeps_the_earlier_mounts_position() {
        // v4's insertion-ordered `Map`: `set(id, item)` on an existing id
        // REPLACES in place rather than appending, so a shadowed item does not
        // jump to the end of the pool.
        let out = merge_mounts(&ids(&["mp-1", "mp-2"]), |mp| {
            if mp == "mp-1" {
                Ok(vec![item("shared", "project"), item("p-only", "project")])
            } else {
                Ok(vec![item("shared", "group")])
            }
        });
        assert_eq!(titles(&out), vec!["shared (group)", "p-only (project)"]);
    }
}
