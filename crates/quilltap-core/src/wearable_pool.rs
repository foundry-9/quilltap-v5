//! The Wearable Pool — everything one character can reach, read once per
//! request (v4 `lib/wardrobe/pool.ts`, `9ecebf986`; the merge helpers of
//! `lib/wardrobe/wearable-pool.ts` stay beside it). P4.D279, keystone C1 §1.
//!
//! The four tiers a character dresses from, in precedence order:
//! **character > group > project > general**. The character's own vault, the
//! stores of every group the character belongs to (resolved here, per
//! character — never per chat), the chat's project stores (passed in, or
//! resolved from `chat_id` behind the project roster), and Quilltap General.
//!
//! Every lookup the wardrobe needs is a walk over this in-memory pool: an item
//! by id, a composite's components to any depth, an item by title, "is this
//! mine". Nothing here re-reads a tier, so a two-level composite costs one read
//! per tier rather than a vault read per nesting level.
//!
//! A batch that dresses a whole cast against one chat shares the
//! character-independent tiers (project + General) through a
//! [`SharedTierLoader`], so they are read once for everyone.
//!
//! Items are the read-path `serde_json::Value`s the vault readers emit, each
//! tagged `origin` LAST (`{ ...item, origin }`). The pool's `by_id` is a
//! `HashMap` — the same [`crate::slot_ops::WearableLookup`] the slot math
//! walks — with the shadow (= insertion) order kept beside it
//! ([`WearablePool::by_id_ordered`]); the crate has no `indexmap`.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use serde_json::Value;

use crate::db::archetype_wardrobe::{
    general_wardrobe_origin, pool_project_origin, with_origin, WardrobeOrigin, WardrobeOriginScope,
};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::tiered_mount_pool::resolve_group_mounts_for_character;
use crate::db::wardrobe_read::{find_by_character_id, read_shared_tiers};
use crate::db::{chats_read, instance_settings, DbError};
use crate::project_roster_access::roster_gated_project_id;
use crate::tools::wardrobe_shared::resolve_project_mount_point_ids;

/// v4 `WardrobeTierMounts` — the shared mounts in scope for one character:
/// `groupMountPointIds` (weakest first) + `projectMountPointIds`. The SAME two
/// fields as the carrier the tiers module always had, so the one type serves
/// both names.
pub type WardrobeTierMounts = crate::wardrobe_tiers::SharedWardrobeTiers;

/// v4 `SharedTierItems` — the character-independent tiers, archived included,
/// each item tagged with its origin.
#[derive(Debug, Clone, Default)]
pub struct SharedTierItems {
    pub general: Vec<Value>,
    pub project: Vec<Value>,
}

/// v4 `WearablePool`.
#[derive(Debug, Clone)]
pub struct WearablePool {
    pub character_id: String,
    pub tiers: WardrobeTierMounts,
    /// Every reachable item, archived included, shadowed character > group >
    /// project > general.
    by_id: HashMap<String, Value>,
    /// The shadow order (v4's `Map` insertion order: a shadowing item keeps the
    /// weaker tier's position).
    order: Vec<String>,
    own_ids: HashSet<String>,
    /// `[general, project, group, own]` — weakest first.
    layers_weakest_first: [Vec<Value>; 4],
    wearable_cache: OnceCell<Vec<Value>>,
}

fn item_id(item: &Value) -> &str {
    item.get("id").and_then(Value::as_str).unwrap_or_default()
}

fn item_title_lower(item: &Value) -> String {
    item.get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase()
}

/// v4 `shadow(tiers)` — weakest first into one id map; `set` keeps the FIRST
/// insertion position and replaces the value.
fn shadow(tiers: &[&[Value]]) -> (HashMap<String, Value>, Vec<String>) {
    let mut by_id: HashMap<String, Value> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for tier in tiers {
        for item in tier.iter() {
            let id = item_id(item).to_string();
            if !by_id.contains_key(&id) {
                order.push(id.clone());
            }
            by_id.insert(id, item.clone());
        }
    }
    (by_id, order)
}

impl WearablePool {
    /// Every reachable item, archived included (v4 `byId`) — the lookup the
    /// slot math and `expand_composites` walk.
    pub fn by_id(&self) -> &HashMap<String, Value> {
        &self.by_id
    }

    /// `byId`'s items in v4's `Map` insertion (= shadow) order.
    pub fn by_id_ordered(&self) -> impl Iterator<Item = &Value> {
        self.order.iter().filter_map(|id| self.by_id.get(id))
    }

    /// What the character can actually wear (v4 `wearable()`): archived items
    /// dropped from each tier *before* shadowing, so an archived personal copy
    /// never hides a shared item. Filter (e.g. `isDefault`) after this, never
    /// before. Memoised.
    pub fn wearable(&self) -> &[Value] {
        self.wearable_cache.get_or_init(|| {
            let filtered: Vec<Vec<Value>> = self
                .layers_weakest_first
                .iter()
                .map(|tier| {
                    tier.iter()
                        .filter(|item| !is_archived_truthy(item))
                        .cloned()
                        .collect()
                })
                .collect();
            let refs: Vec<&[Value]> = filtered.iter().map(Vec::as_slice).collect();
            let (by_id, order) = shadow(&refs);
            order
                .iter()
                .filter_map(|id| by_id.get(id).cloned())
                .collect()
        })
    }

    pub fn get(&self, id: &str) -> Option<&Value> {
        self.by_id.get(id)
    }

    /// The ids found, in the order asked; unknown ids and duplicates skipped
    /// (v4 `getMany`).
    pub fn get_many(&self, ids: &[String]) -> Vec<&Value> {
        let mut out = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for id in ids {
            if !seen.insert(id.as_str()) {
                continue;
            }
            if let Some(item) = self.by_id.get(id) {
                out.push(item);
            }
        }
        out
    }

    /// Case-insensitive title match (v4 `findByTitle`): the character's own
    /// items first (archived INCLUDED), then the wearable shared set — so an
    /// archived SHARED item is never found by title. The search term is JS-
    /// trimmed; the titles are not.
    pub fn find_by_title(&self, title: &str) -> Option<&Value> {
        let wanted = crate::jsstr::js_trim(title).to_lowercase();
        if wanted.is_empty() {
            return None;
        }
        let own = &self.layers_weakest_first[3];
        if let Some(found) = own.iter().find(|item| item_title_lower(item) == wanted) {
            // The own layer's items are the same `Value`s `by_id` holds for
            // those ids (own is the strongest tier), so answer through `by_id`.
            return self.by_id.get(item_id(found));
        }
        self.wearable()
            .iter()
            .find(|item| !self.own_ids.contains(item_id(item)) && item_title_lower(item) == wanted)
    }

    /// The one "is this mine" predicate (v4 `owns`): the item lives in this
    /// character's vault — `item.characterId === characterId` AND an own-layer
    /// id.
    pub fn owns(&self, item: &Value) -> bool {
        item.get("characterId").and_then(Value::as_str) == Some(self.character_id.as_str())
            && self.own_ids.contains(item_id(item))
    }

    /// `byId.size`.
    pub fn reachable(&self) -> usize {
        self.by_id.len()
    }
}

/// v4 `buildWearablePool(characterId, tierMounts, { own, group, project,
/// general })` — PURE. The loader and the harness both come through here, so
/// the precedence and archived rules live once. Every item is taken as given
/// (already origin-tagged by the caller).
pub fn build_wearable_pool(
    character_id: &str,
    tiers: WardrobeTierMounts,
    own: Vec<Value>,
    group: Vec<Value>,
    project: Vec<Value>,
    general: Vec<Value>,
) -> WearablePool {
    let own_ids: HashSet<String> = own.iter().map(|i| item_id(i).to_string()).collect();
    let layers_weakest_first = [general, project, group, own];
    let refs: Vec<&[Value]> = layers_weakest_first.iter().map(Vec::as_slice).collect();
    let (by_id, order) = shadow(&refs);
    WearablePool {
        character_id: character_id.to_string(),
        tiers,
        by_id,
        order,
        own_ids,
        layers_weakest_first,
        wearable_cache: OnceCell::new(),
    }
}

/// v4 `loadSharedTiers(repos, projectMountPointIds, projectOrigin?)` — read
/// Quilltap General and the given project stores, archived included. A
/// POOL-level project origin has `id: null` (`pool_project_origin`) — the pool
/// never knows which project. `Err` only from the General mount lookup
/// (`getGeneralMountPointId`); the tier reads themselves never fail.
pub fn load_shared_tiers(
    main: &Connection,
    docs: &DocMountDocumentsRepository,
    project_mount_point_ids: &[String],
    project_origin: Option<&WardrobeOrigin>,
) -> Result<SharedTierItems, DbError> {
    let general_mounts: Vec<String> = instance_settings::get_general_mount_point_id(main)?
        .into_iter()
        .collect();
    let general = read_shared_tiers(docs, &general_mounts, true, &|_| general_wardrobe_origin());
    let project_origin = project_origin.cloned().unwrap_or_else(pool_project_origin);
    let project = read_shared_tiers(docs, project_mount_point_ids, true, &|_| {
        project_origin.clone()
    });
    Ok(SharedTierItems { general, project })
}

/// v4 `createSharedTierLoader(repos, projectMountPointIds)` — a memoised
/// shared-tier read for a batch: the first character to ask pays for it,
/// everyone after reuses it. Lazy, so a batch that never needs a pool never
/// reads a tier. A failure WARNs `[WearablePool] Failed to read shared wardrobe
/// tiers; continuing without them` and answers empty tiers.
#[derive(Debug)]
pub struct SharedTierLoader {
    project_mount_point_ids: Vec<String>,
    cell: OnceCell<SharedTierItems>,
}

impl SharedTierLoader {
    pub fn new(project_mount_point_ids: &[String]) -> Self {
        SharedTierLoader {
            project_mount_point_ids: project_mount_point_ids.to_vec(),
            cell: OnceCell::new(),
        }
    }

    pub fn get(&self, main: &Connection, docs: &DocMountDocumentsRepository) -> &SharedTierItems {
        self.cell.get_or_init(|| {
            match load_shared_tiers(main, docs, &self.project_mount_point_ids, None) {
                Ok(tiers) => tiers,
                Err(error) => {
                    tracing::warn!(
                        projectMountCount = self.project_mount_point_ids.len(),
                        context = "wardrobe",
                        error = %crate::db::fallback::error_text(&error),
                        "[WearablePool] Failed to read shared wardrobe tiers; continuing without them"
                    );
                    SharedTierItems::default()
                }
            }
        })
    }
}

/// v4 `resolveProjectTierForChat(repos, chatId, characterId, { operator })`
/// (`pool.ts:149-180`; MOVED here from `shared-tiers.ts`, which `9ecebf986`
/// deleted) — the chat's project stores as this character may see them: none
/// when the chat has no project, or (unless `operator`) when the character is
/// off the project's roster (`crate::project_roster_access`). Fails soft to
/// `[]`.
///
/// `chat_id` / `character_id` are JS-truthiness arguments: an empty or absent
/// chat id answers `[]` with no line; an empty character admits.
///
/// The chat is read through the fallback `find_by_id_or_none` — v4's
/// `chats.findById` is the fallback `_findById`, so a FAILED read logs `Error
/// finding entity by ID {collection: chats, id}` and answers `null` → `[]`.
/// v4's own `catch` around that read, WARN `[WearablePool] Project lookup for
/// chat failed`, is therefore UNREACHABLE on a repository failure (the P4.90 /
/// P4.D225 "catch behind a fallback `safeQuery`" class) and is recorded, not
/// ported.
pub fn resolve_project_tier_for_chat(
    main: &Connection,
    mount: &Connection,
    chat_id: Option<&str>,
    character_id: Option<&str>,
    operator: bool,
) -> Vec<String> {
    let Some(chat_id) = chat_id.filter(|c| !c.is_empty()) else {
        return Vec::new();
    };
    let Some(project_id) = crate::db::fallback::find_by_id_or_none("chats", chat_id, || {
        chats_read::find_by_id(main, chat_id)
    })
    .as_ref()
    .and_then(|chat| chat.get("projectId"))
    .and_then(Value::as_str)
    .filter(|p| !p.is_empty())
    .map(str::to_string) else {
        return Vec::new();
    };
    let gated = if operator {
        Some(project_id.clone())
    } else {
        roster_gated_project_id(
            main,
            mount,
            Some(&project_id),
            character_id.filter(|c| !c.is_empty()),
        )
    };
    let Some(gated) = gated else {
        tracing::debug!(
            chatId = %chat_id,
            projectId = %project_id,
            characterId = %character_id.unwrap_or_default(),
            context = "wardrobe",
            "[WearablePool] Character off project roster — project wardrobe withheld"
        );
        return Vec::new();
    };
    resolve_project_mount_point_ids(mount, Some(&gated))
}

/// v4 `LoadWearablePoolOptions`.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadWearablePoolOptions<'a> {
    /// The operator is choosing on the character's behalf (the Salon's outfit
    /// dialog), so the project roster does not apply. Character tool calls
    /// never set this. Only consulted when the project tier is resolved from
    /// `chat_id`.
    pub operator: bool,
    /// Resolve the project tier from this chat when no project mounts are
    /// passed.
    pub chat_id: Option<&'a str>,
    /// Share the character-independent tiers across a batch.
    pub shared_tiers: Option<&'a SharedTierLoader>,
    /// The character's own items, already read by the caller (the vault
    /// writer's cycle check has just read the folder). Skips the vault read.
    pub own_items: Option<&'a [Value]>,
}

/// v4 `loadWearablePool(repos, characterId, projectMountPointIds?, opts)` —
/// load a character's wearable pool. Pass the project tier when the caller
/// already holds it (even `[]`); otherwise give `chat_id` and it is resolved
/// (roster-gated) here. The group tier is always resolved here. Each tier
/// fails soft on its own: one unreadable store narrows the pool, it never
/// empties it. Never fails.
///
/// Lines: WARN `[WearablePool] Failed to read the character wardrobe; using
/// shared tiers alone` when the own read fails; DEBUG `[WearablePool] Loaded
/// wearable pool` always. v4's WARN `[WearablePool] Failed to read the group
/// wardrobe tier; skipping it` sits around `readSharedTiers`, which catches
/// every mount's failure itself — UNREACHABLE through v4's real code, pinned
/// NEGATIVE, not ported.
pub fn load_wearable_pool(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    project_mount_point_ids: Option<&[String]>,
    opts: LoadWearablePoolOptions<'_>,
) -> WearablePool {
    let docs = DocMountDocumentsRepository::new(mount);
    let project_ids: Vec<String> = match project_mount_point_ids {
        Some(ids) => ids.to_vec(),
        None => resolve_project_tier_for_chat(
            main,
            mount,
            opts.chat_id,
            Some(character_id),
            opts.operator,
        ),
    };

    let groups = resolve_group_mounts_for_character(main, mount, character_id);
    let own_loader;
    let shared: SharedTierItems = match opts.shared_tiers {
        Some(loader) => loader.get(main, &docs).clone(),
        None => {
            own_loader = SharedTierLoader::new(&project_ids);
            own_loader.get(main, &docs).clone()
        }
    };
    let own: Vec<Value> = match opts.own_items {
        Some(items) => items.to_vec(),
        None => match find_by_character_id(main, &docs, character_id, true) {
            Ok(items) => items,
            Err(error) => {
                tracing::warn!(
                    characterId = %character_id,
                    context = "wardrobe",
                    error = %crate::db::fallback::error_text(&error),
                    "[WearablePool] Failed to read the character wardrobe; using shared tiers alone"
                );
                Vec::new()
            }
        },
    };

    let mut group_origin_by_mount: HashMap<String, WardrobeOrigin> = HashMap::new();
    let mut group_mount_point_ids: Vec<String> = Vec::new();
    for g in &groups {
        for mp in &g.mount_point_ids {
            if !group_origin_by_mount.contains_key(mp) {
                group_mount_point_ids.push(mp.clone());
            }
            group_origin_by_mount.insert(
                mp.clone(),
                WardrobeOrigin::new(WardrobeOriginScope::Group, &g.group.id, &g.group.name),
            );
        }
    }
    let group = read_shared_tiers(&docs, &group_mount_point_ids, true, &|mp| {
        group_origin_by_mount
            .get(mp)
            .cloned()
            .unwrap_or_else(pool_project_origin)
    });

    // `{ ...item, characterId, origin: ownOrigin }` — the name is EMPTY.
    let own_origin = WardrobeOrigin::new(WardrobeOriginScope::Character, character_id, "");
    let own_tagged: Vec<Value> = with_origin(
        own.into_iter()
            .map(|mut item| {
                if let Some(obj) = item.as_object_mut() {
                    obj.insert(
                        "characterId".to_string(),
                        Value::String(character_id.to_string()),
                    );
                }
                item
            })
            .collect(),
        &own_origin,
    );

    let own_count = own_tagged.len();
    let group_count = group.len();
    let project_count = shared.project.len();
    let general_count = shared.general.len();
    let pool = build_wearable_pool(
        character_id,
        WardrobeTierMounts {
            group_mount_point_ids: group_mount_point_ids.clone(),
            project_mount_point_ids: project_ids.clone(),
        },
        own_tagged,
        group,
        shared.project,
        shared.general,
    );

    tracing::debug!(
        characterId = %character_id,
        ownCount = own_count,
        groupCount = group_count,
        projectCount = project_count,
        generalCount = general_count,
        reachable = pool.reachable(),
        groupMountCount = group_mount_point_ids.len(),
        projectMountCount = project_ids.len(),
        context = "wardrobe",
        "[WearablePool] Loaded wearable pool"
    );
    pool
}

/// v4 `componentGraph(pool, rootIds)` — every item reachable from a set of
/// root ids, transitively through `componentItemIds`: a pure BFS over the pool
/// that tolerates cycles and unknowns. Discovery order (v4's `Map` insertion
/// order). For callers that want a small lookup (a composite and its parts)
/// rather than the whole pool — [`component_graph_lookup`] gives it as the
/// slot math's map.
pub fn component_graph(pool: &WearablePool, root_ids: &[String]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: std::collections::VecDeque<String> = root_ids.iter().cloned().collect();
    while let Some(id) = queue.pop_front() {
        if seen.contains(&id) {
            continue;
        }
        let Some(item) = pool.get(&id) else {
            continue;
        };
        seen.insert(id);
        if let Some(components) = item.get("componentItemIds").and_then(Value::as_array) {
            for c in components {
                if let Some(cid) = c.as_str() {
                    queue.push_back(cid.to_string());
                }
            }
        }
        out.push(item.clone());
    }
    out
}

/// [`component_graph`] as an `id → item` map (the [`crate::slot_ops::WearableLookup`]
/// shape).
pub fn component_graph_lookup(pool: &WearablePool, root_ids: &[String]) -> HashMap<String, Value> {
    component_graph(pool, root_ids)
        .into_iter()
        .map(|item| (item_id(&item).to_string(), item))
        .collect()
}

/// v4 `loadCastPools(repos, projectId, characterIds)` — one pool per cast
/// member for a chat, sharing the project and General reads across the cast.
/// Each character still sees only their own groups. The operator is reading
/// (the outfit summary), so the project roster doesn't apply. Answered in
/// `character_ids` order (v4's `Map`).
pub fn load_cast_pools(
    main: &Connection,
    mount: &Connection,
    project_id: Option<&str>,
    character_ids: &[String],
) -> Vec<(String, WearablePool)> {
    let project_mount_point_ids = resolve_project_mount_point_ids(mount, project_id);
    let shared = SharedTierLoader::new(&project_mount_point_ids);
    character_ids
        .iter()
        .map(|character_id| {
            (
                character_id.clone(),
                load_wearable_pool(
                    main,
                    mount,
                    character_id,
                    Some(&project_mount_point_ids),
                    LoadWearablePoolOptions {
                        shared_tiers: Some(&shared),
                        ..LoadWearablePoolOptions::default()
                    },
                ),
            )
        })
        .collect()
}

// ===========================================================================
// wearable-pool.ts — the client-safe merge (kept by v4)
// ===========================================================================

/// v4's `!item.archivedAt` — an item is archived when `archivedAt` is *truthy*:
/// a non-empty string, `true`, or a non-zero number. `null`, absent, `false`,
/// `0` and `""` all read as active.
///
/// The canonical copy: [`crate::db::wardrobe_read`] and
/// [`crate::db::archetype_wardrobe`] both filter with it, and so does
/// [`merge_wearable_pool`], so the tiers agree on what "archived" means.
pub fn is_archived_truthy(item: &Value) -> bool {
    match item.get("archivedAt") {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => false,
    }
}

/// v4 `mergeWearableTiers(tiersWeakestFirst)` — merge tiers given weakest-first
/// (General, project, group, then the character's own) into the wearable list.
/// Archived items are dropped from each tier *as it merges*, so an archived
/// personal copy never hides the shared item it once overrode — the shared one
/// resurfaces. Order is v4's insertion-ordered `Map`: a shadowing item replaces
/// the value *at the weaker item's position*.
pub fn merge_wearable_tiers(tiers_weakest_first: &[&[Value]]) -> Vec<Value> {
    let mut order: Vec<Value> = Vec::new();
    let mut index_by_id: HashMap<String, usize> = HashMap::new();
    for tier in tiers_weakest_first {
        for item in tier.iter() {
            if is_archived_truthy(item) {
                continue;
            }
            let id = item_id(item).to_string();
            if let Some(&i) = index_by_id.get(&id) {
                order[i] = item.clone();
            } else {
                index_by_id.insert(id, order.len());
                order.push(item.clone());
            }
        }
    }
    order
}

/// v4 `mergeWearablePool(shared, own)` — [`merge_wearable_tiers`] for one
/// already-flattened shared list under the character's own.
///
/// Precedence is character > shared: a personal item with the same id as a
/// shared one shadows it entirely (that's how a character keeps a private
/// variant of a house garment, and how they opt *out* of a shared default by
/// holding a copy with `isDefault: false`). **Callers who need archived items
/// (the equip path) must not use this helper.**
pub fn merge_wearable_pool(shared: &[Value], own: &[Value]) -> Vec<Value> {
    merge_wearable_tiers(&[shared, own])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, extra: Value) -> Value {
        let mut v = json!({ "id": id, "title": id, "types": ["top"], "archivedAt": null });
        if let (Some(obj), Some(ex)) = (v.as_object_mut(), extra.as_object()) {
            for (k, val) in ex {
                obj.insert(k.clone(), val.clone());
            }
        }
        v
    }

    fn ids(items: &[Value]) -> Vec<&str> {
        items
            .iter()
            .filter_map(|i| i.get("id").and_then(Value::as_str))
            .collect()
    }

    /// v4 pool test: "applies precedence character > project > general".
    #[test]
    fn character_shadows_shared_in_place() {
        let shared = vec![
            item("livery", json!({ "title": "shared livery" })),
            item("hat", json!({})),
        ];
        let own = vec![
            item("livery", json!({ "title": "own livery" })),
            item("boots", json!({})),
        ];
        let pool = merge_wearable_pool(&shared, &own);
        assert_eq!(ids(&pool), vec!["livery", "hat", "boots"]);
        assert_eq!(
            pool[0].get("title").and_then(Value::as_str),
            Some("own livery")
        );
    }

    /// v4 `wearable-pool.ts`: archived dropped PER TIER as it merges, so an
    /// archived personal copy UNMASKS the shared item (v4 `9ecebf986` — the old
    /// merge filtered last and removed the entry entirely).
    #[test]
    fn an_archived_personal_override_unmasks_the_shared_item() {
        let shared = vec![item("livery", json!({ "title": "shared" }))];
        let own = vec![item(
            "livery",
            json!({ "title": "own", "archivedAt": "2026-02-02T00:00:00.000Z" }),
        )];
        let pool = merge_wearable_pool(&shared, &own);
        assert_eq!(ids(&pool), vec!["livery"]);
        assert_eq!(pool[0]["title"], "shared");
    }

    #[test]
    fn drops_archived_shared_items_and_empty_string_is_active() {
        let shared = vec![
            item("cloak", json!({ "archivedAt": "2026-02-02T00:00:00.000Z" })),
            item("coat", json!({ "archivedAt": "" })),
        ];
        assert_eq!(ids(&merge_wearable_pool(&shared, &[])), vec!["coat"]);
    }

    fn pool() -> WearablePool {
        let general = vec![
            item("g1", json!({ "title": "Top Hat", "characterId": null })),
            item("shared", json!({ "title": "Livery", "characterId": null })),
            item(
                "garch",
                json!({ "title": "Old Coat", "characterId": null, "archivedAt": "2026-01-01T00:00:00.000Z" }),
            ),
        ];
        let project = vec![item("p1", json!({ "title": "Badge", "characterId": null }))];
        let group = vec![item(
            "shared",
            json!({ "title": "Group Livery", "characterId": null }),
        )];
        let own = vec![
            item(
                "shared",
                json!({ "title": "My Livery", "characterId": "c1", "archivedAt": "2026-01-01T00:00:00.000Z" }),
            ),
            item(
                "o1",
                json!({ "title": "Boots", "characterId": "c1", "componentItemIds": ["g1", "p1", "ghost"] }),
            ),
        ];
        build_wearable_pool(
            "c1",
            WardrobeTierMounts::default(),
            own,
            group,
            project,
            general,
        )
    }

    #[test]
    fn by_id_shadows_in_place_and_wearable_drops_archived_per_tier() {
        let p = pool();
        let order: Vec<&str> = p.by_id_ordered().map(item_id).collect();
        assert_eq!(order, vec!["g1", "shared", "garch", "p1", "o1"]);
        assert_eq!(p.get("shared").unwrap()["title"], "My Livery");
        // Archived own copy dropped BEFORE shadowing → the group's live copy
        // resurfaces; the archived General coat is gone.
        let wearable: Vec<&str> = p.wearable().iter().map(item_id).collect();
        assert_eq!(wearable, vec!["g1", "shared", "p1", "o1"]);
        assert_eq!(
            p.wearable()
                .iter()
                .find(|i| item_id(i) == "shared")
                .unwrap()["title"],
            "Group Livery"
        );
    }

    #[test]
    fn get_many_find_by_title_owns_and_graph() {
        let p = pool();
        let many: Vec<&str> = p
            .get_many(&["p1".into(), "nope".into(), "g1".into(), "p1".into()])
            .into_iter()
            .map(item_id)
            .collect();
        assert_eq!(many, vec!["p1", "g1"]);
        // Own first, archived included.
        assert_eq!(item_id(p.find_by_title("  my livery ").unwrap()), "shared");
        // Shared only when wearable: the archived General coat is never found.
        assert!(p.find_by_title("Old Coat").is_none());
        assert_eq!(item_id(p.find_by_title("top hat").unwrap()), "g1");
        assert!(p.find_by_title("   ").is_none());
        assert!(p.owns(p.get("o1").unwrap()));
        assert!(!p.owns(p.get("g1").unwrap()));
        let graph: Vec<String> = component_graph(&p, &["o1".into(), "o1".into()])
            .iter()
            .map(|i| item_id(i).to_string())
            .collect();
        assert_eq!(graph, vec!["o1", "g1", "p1"]);
    }
}
