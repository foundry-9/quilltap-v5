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

/// v4 `WardrobeRepository.findArchetypesInMountsAttributed` (`cc80dc89d`) —
/// the group tier of a character's wardrobe with each item tagged by the group
/// it hangs in, for the dialog's origin chip. Reads every group's mounts in the
/// order given (resolve them with
/// [`super::tiered_mount_pool::resolve_group_mounts_for_character`]) and
/// resolves an id collision exactly as [`find_archetypes_in_mounts`] does over
/// the flattened list — a later mount's copy shadows an earlier one — so the
/// item that wins here is the item that wins there, and it carries ITS OWN
/// group's origin (the tagged value replaces the earlier one in place, v4's
/// `Map.set`).
///
/// `[]` for no groups BEFORE anything (no line). A mount that can't be read is
/// logged (with its `groupId`) and skipped. v4's `safeQuery` wrapper (`Error
/// finding attributed group wardrobe items { groupCount, includeArchived }`) is
/// unreachable — every read inside it is caught per mount, as in the flat
/// sibling — so this answers `Ok` always.
pub fn find_archetypes_in_mounts_attributed(
    docs: &DocMountDocumentsRepository,
    groups: &[GroupMounts],
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    Ok(merge_groups_attributed(groups, |mount_point_id| {
        read_shared_wardrobe(docs, mount_point_id, include_archived)
    }))
}

/// The attributed merge over an injected reader (the [`merge_mounts`] seam).
fn merge_groups_attributed<F>(groups: &[GroupMounts], mut read: F) -> Vec<Value>
where
    F: FnMut(&str) -> Result<Vec<Value>, DbError>,
{
    if groups.is_empty() {
        return Vec::new();
    }
    let mut acc = OrderedById::new();
    for g in groups {
        let origin = WardrobeOrigin::new(WardrobeOriginScope::Group, &g.group.id, &g.group.name);
        for mount_point_id in &g.mount_point_ids {
            match read(mount_point_id) {
                Ok(items) => {
                    for item in with_origin(items, &origin) {
                        acc.upsert(item);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        mountPointId = %mount_point_id, groupId = %g.group.id,
                        context = "wardrobe", error = %crate::db::fallback::error_text(&e),
                        "Failed to read shared wardrobe tier; skipping"
                    );
                }
            }
        }
    }
    tracing::debug!(
        groupCount = groups.len(),
        itemCount = acc.order.len(),
        context = "wardrobe",
        "Attributed group wardrobe read"
    );
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

    // === `cc80dc89d`: the attributed read — v4's four
    // `findArchetypesInMountsAttributed` cases, over the same reader seam. ===

    fn gm(id: &str, name: &str, mounts: &[&str]) -> GroupMounts {
        GroupMounts {
            group: super::super::tiered_mount_pool::GroupRef {
                id: id.to_string(),
                name: name.to_string(),
            },
            mount_point_ids: ids(mounts),
        }
    }

    #[test]
    fn attributed_tags_every_item_with_the_group_whose_store_it_hangs_in() {
        let (out, lines) = crate::test_support::captured_with(|| {
            merge_groups_attributed(
                &[
                    gm("G1", "The Sisters", &["m-sisters"]),
                    gm("G2", "The Regiment", &["m-regiment"]),
                ],
                |mp| {
                    Ok(vec![if mp == "m-sisters" {
                        item("shawl", "sisters")
                    } else {
                        item("kit", "regiment")
                    }])
                },
            )
        });
        assert_eq!(
            out,
            vec![
                json!({ "id": "shawl", "characterId": null, "title": "shawl (sisters)",
                        "origin": { "scope": "group", "id": "G1", "name": "The Sisters" } }),
                json!({ "id": "kit", "characterId": null, "title": "kit (regiment)",
                        "origin": { "scope": "group", "id": "G2", "name": "The Regiment" } }),
            ]
        );
        // `origin` is the LAST key (v4's object spread).
        assert_eq!(
            out[0].as_object().unwrap().keys().next_back().unwrap(),
            "origin"
        );
        assert_eq!(
            lines,
            vec!["DEBUG quilltap_core::db::archetype_wardrobe Attributed group wardrobe read groupCount=2 itemCount=2 context=wardrobe".to_string()]
        );
    }

    #[test]
    fn attributed_collision_resolves_as_the_flat_read_and_the_winner_keeps_its_own_origin() {
        let reader = |mp: &str| {
            Ok(vec![if mp == "m-sisters" {
                item("livery", "sisters")
            } else {
                item("livery", "regiment")
            }])
        };
        let flat = merge_mounts(&ids(&["m-sisters", "m-regiment"]), reader);
        let attributed = merge_groups_attributed(
            &[
                gm("G1", "The Sisters", &["m-sisters"]),
                gm("G2", "The Regiment", &["m-regiment"]),
            ],
            reader,
        );
        assert_eq!(attributed.len(), 1);
        assert_eq!(attributed[0]["title"], flat[0]["title"]);
        assert_eq!(attributed[0]["title"], "livery (regiment)");
        assert_eq!(
            attributed[0]["origin"],
            json!({ "scope": "group", "id": "G2", "name": "The Regiment" })
        );
    }

    #[test]
    fn attributed_skips_an_unreadable_store_with_its_group_id() {
        let (out, lines) = crate::test_support::captured_with(|| {
            merge_groups_attributed(
                &[
                    gm("G1", "The Sisters", &["m-broken"]),
                    gm("G2", "The Regiment", &["m-regiment"]),
                ],
                |mp| {
                    if mp == "m-broken" {
                        return Err(DbError::Internal("store offline".into()));
                    }
                    Ok(vec![item("kit", "regiment")])
                },
            )
        });
        assert_eq!(titles(&out), vec!["kit (regiment)"]);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            lines[0],
            "WARN quilltap_core::db::archetype_wardrobe Failed to read shared wardrobe tier; skipping mountPointId=m-broken groupId=G1 context=wardrobe error=store offline"
        );
        assert!(lines[1].contains("Attributed group wardrobe read groupCount=2 itemCount=1"));
    }

    #[test]
    fn attributed_reads_nothing_and_logs_nothing_for_an_empty_group_tier() {
        let mut calls = 0usize;
        let (out, lines) = crate::test_support::captured_with(|| {
            merge_groups_attributed(&[], |_| {
                calls += 1;
                Ok(Vec::new())
            })
        });
        assert!(out.is_empty());
        assert_eq!(calls, 0);
        assert!(lines.is_empty(), "{lines:?}");
    }

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
