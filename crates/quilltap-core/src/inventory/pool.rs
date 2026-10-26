//! Inventory Pool — everything one character can reach, read once per
//! request (v4 `lib/inventory/pool.ts`, `4fbdad186` + `102a23648` +
//! `7263105c6`'s data half; P4.D324).
//!
//! The four tiers in precedence order, **character > group > project >
//! general**: the character's own vault, the stores of every group the
//! character belongs to, the chat's (or a given project's) project stores,
//! and Quilltap General. Archived objects are dropped per tier *before*
//! shadowing, so an archived personal copy never hides a shared one. Every
//! lookup outside the writer is a walk over this.
//!
//! An archetype may live in a tier this pool cannot see (a character's
//! instance of a project archetype, read outside that project). Those parents
//! are fetched instance-wide in one extra read, so an instance resolves
//! wherever it is looked at; only a parent that exists nowhere resolves as
//! `parentMissing`.
//!
//! `consulted_mounts` / `unread_mounts` (`7263105c6`) say where the pool
//! looked and which tier reads failed — data the title lint (P4.D340) refuses
//! on; fail-soft readers ignore them.
//!
//! The pool's records carry `origin` (the tier's tag) and `mountPointId` as
//! map keys appended after the file's own (v4 `{...record, origin,
//! mountPointId}`), so [`resolve`](InventoryPool::resolve) sees them where v4
//! does. v4's `Promise.all` of the four tier reads is order-free; they run
//! sequentially here in v4's listed order (own, group, project, general).

use std::cell::{OnceCell, RefCell};
use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::Value;

use super::dependents::find_records_anywhere;
use super::location::{general_inventory_origin, InventoryLocation};
use super::resolve::{is_kin, resolve_item};
use super::types::{
    InventoryFields, InventoryKind, InventoryOrigin, InventoryRecord, InventoryScope,
    ResolvedInventoryItem,
};
use crate::db::inventory_writes::read_mount_inventory;
use crate::db::item_folder_ops::{VaultFolderMount, VaultFolderMountScope};
use crate::db::projects::ProjectsRepository;
use crate::db::tiered_mount_pool::resolve_group_mounts_for_character;
use crate::db::{characters_read, chats_read, instance_settings, DbError};
use crate::tools::wardrobe_shared::resolve_project_mount_point_ids;

/// The shared tiers' mounts (v4 `InventoryTierMounts`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryTierMounts {
    pub group_mount_point_ids: Vec<String>,
    pub project_mount_point_ids: Vec<String>,
}

/// A tier mount the pool could not read, and why (v4 `UnreadInventoryMount`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadInventoryMount {
    pub mount_point_id: String,
    pub error: String,
}

/// An insertion-ordered id map with V8 `Map` semantics: a re-set keeps the
/// FIRST position and takes the LATER value.
#[derive(Debug, Clone, Default)]
pub struct RecordMap {
    entries: Vec<(String, InventoryRecord)>,
}

impl RecordMap {
    fn set(&mut self, record: InventoryRecord) {
        let id = record.id().to_string();
        match self.entries.iter_mut().find(|(k, _)| *k == id) {
            Some((_, slot)) => *slot = record,
            None => self.entries.push((id, record)),
        }
    }
    pub fn get(&self, id: &str) -> Option<&InventoryRecord> {
        self.entries.iter().find(|(k, _)| k == id).map(|(_, v)| v)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// `Array.from(byId.values())`, in Map order.
    pub fn values(&self) -> impl Iterator<Item = &InventoryRecord> {
        self.entries.iter().map(|(_, v)| v)
    }
}

fn shadow<'a>(layers: impl IntoIterator<Item = &'a [InventoryRecord]>) -> RecordMap {
    let mut by_id = RecordMap::default();
    for layer in layers {
        for record in layer {
            by_id.set(record.clone());
        }
    }
    by_id
}

/// The four tiers' records, each tagged (`origin`, `mountPointId`).
#[derive(Debug, Clone, Default)]
pub struct InventoryLayers {
    pub own: Vec<InventoryRecord>,
    pub group: Vec<InventoryRecord>,
    pub project: Vec<InventoryRecord>,
    pub general: Vec<InventoryRecord>,
}

/// Everything one holder can reach (v4 `InventoryPool`).
#[derive(Debug)]
pub struct InventoryPool {
    /// The character whose reach this is; `None` for a shared tier's pool.
    pub character_id: Option<String>,
    pub tiers: InventoryTierMounts,
    /// Every mount this pool read (or tried to).
    pub consulted_mounts: Vec<String>,
    /// Mounts whose records are missing because the read failed.
    pub unread_mounts: Vec<UnreadInventoryMount>,
    /// Every reachable record, archived included, shadowed by precedence.
    pub by_id: RecordMap,
    /// The holder's own tier's records (archived included).
    pub own: Vec<InventoryRecord>,
    weakest_first: [Vec<InventoryRecord>; 4],
    external_parents: RecordMap,
    reachable_cache: OnceCell<Vec<InventoryRecord>>,
    warned: RefCell<HashSet<String>>,
}

/// Build a pool from tiers already in hand (v4 `buildInventoryPool`). Pure
/// but for the parent map the loader passes in.
pub fn build_inventory_pool(
    character_id: Option<String>,
    tiers: InventoryTierMounts,
    layers: InventoryLayers,
    external_parents: RecordMap,
    unread_mounts: Vec<UnreadInventoryMount>,
    consulted_mounts: Vec<String>,
) -> InventoryPool {
    let weakest_first = [
        layers.general,
        layers.project,
        layers.group,
        layers.own.clone(),
    ];
    let by_id = shadow(weakest_first.iter().map(Vec::as_slice));
    InventoryPool {
        character_id,
        tiers,
        consulted_mounts,
        unread_mounts,
        by_id,
        own: layers.own,
        weakest_first,
        external_parents,
        reachable_cache: OnceCell::new(),
        warned: RefCell::new(HashSet::new()),
    }
}

impl InventoryPool {
    /// Records with archived ones dropped PER TIER before shadowing (cached).
    pub fn reachable(&self) -> &[InventoryRecord] {
        self.reachable_cache.get_or_init(|| {
            let filtered: Vec<Vec<InventoryRecord>> = self
                .weakest_first
                .iter()
                .map(|layer| layer.iter().filter(|r| !r.is_archived()).cloned().collect())
                .collect();
            shadow(filtered.iter().map(Vec::as_slice))
                .values()
                .cloned()
                .collect()
        })
    }

    pub fn get(&self, id: &str) -> Option<&InventoryRecord> {
        self.by_id.get(id)
    }

    /// True when the record lives in the holder's own tier.
    pub fn owns(&self, id: &str) -> bool {
        self.own.iter().any(|r| r.id() == id)
    }

    /// Resolve one record against this pool (and the out-of-pool parents).
    /// An instance whose archetype exists nowhere WARNs once per id.
    pub fn resolve(&self, record: &InventoryRecord) -> ResolvedInventoryItem {
        let item = resolve_item(record, |id| {
            self.by_id.get(id).or_else(|| self.external_parents.get(id))
        });
        if item.parent_missing() == Some(true)
            && self.warned.borrow_mut().insert(record.id().to_string())
        {
            tracing::warn!(
                characterId = self.character_id.as_deref().unwrap_or("null"),
                itemId = %record.id(),
                parentId = record.str_of("parentId"),
                context = "inventory",
                "[InventoryPool] Instance whose archetype exists nowhere; resolved to its own fields"
            );
        }
        item
    }

    /// Resolve the whole reachable (non-archived) pool.
    pub fn resolved(&self) -> Vec<ResolvedInventoryItem> {
        self.reachable().iter().map(|r| self.resolve(r)).collect()
    }

    /// Resolve by id (archived included); `None` when unknown.
    pub fn resolve_id(&self, id: &str) -> Option<ResolvedInventoryItem> {
        self.by_id.get(id).map(|r| self.resolve(r))
    }
}

/// `{...record, origin, mountPointId}`.
fn tag(
    record: InventoryRecord,
    origin: &InventoryOrigin,
    mount_point_id: Option<&str>,
) -> InventoryRecord {
    let mut r = record;
    r.set("origin", origin.to_value());
    match mount_point_id {
        Some(mp) => r.set("mountPointId", Value::String(mp.to_string())),
        // `mountPointId: undefined` — the key is set, the wire drops it; the
        // position matters only if a later spread re-sets it (none does).
        None => r.remove("mountPointId"),
    }
    r
}

/// Read a list of mounts weakest-first, tagging each record (v4 `readTier`).
/// Fails soft per mount, noting each failure in `unread`; every mount is
/// pushed to `consulted` first.
fn read_tier(
    mount: &Connection,
    mount_point_ids: &[String],
    origin_of: impl Fn(&str) -> InventoryOrigin,
    unread: &mut Vec<UnreadInventoryMount>,
    consulted: &mut Vec<String>,
) -> Vec<InventoryRecord> {
    let mut out = Vec::new();
    for mount_point_id in mount_point_ids {
        consulted.push(mount_point_id.clone());
        let m = VaultFolderMount {
            mount_point_id: mount_point_id.clone(),
            scope: VaultFolderMountScope::Project,
            character_id: None,
        };
        match read_mount_inventory(mount, &m) {
            Ok(records) => {
                let origin = origin_of(mount_point_id);
                out.extend(
                    records
                        .into_iter()
                        .map(|r| tag(r, &origin, Some(mount_point_id))),
                );
            }
            Err(error) => {
                let message = crate::db::fallback::error_text(&error);
                unread.push(UnreadInventoryMount {
                    mount_point_id: mount_point_id.clone(),
                    error: message.clone(),
                });
                tracing::warn!(
                    mountPointId = %mount_point_id,
                    context = "inventory",
                    error = %message,
                    "[InventoryPool] Failed to read an Inventory tier; skipping it"
                );
            }
        }
    }
    out
}

/// The archetypes instances in the pool name that the pool itself cannot see
/// (v4 `fetchExternalParents`), tagged with v4's lookup-only origin
/// `{scope: 'project', id: null, name: 'Elsewhere'}` (never in `resolved()`).
/// DEBUG `[InventoryPool] Fetched archetypes outside the pool`.
fn fetch_external_parents(mount: &Connection, layers: &[&[InventoryRecord]]) -> RecordMap {
    let known: HashSet<&str> = layers
        .iter()
        .flat_map(|l| l.iter().map(|r| r.id()))
        .collect();
    let mut missing: Vec<String> = Vec::new();
    for layer in layers {
        for record in layer.iter() {
            if let Some(parent) = record.parent_id() {
                if !known.contains(parent) && !missing.iter().any(|m| m == parent) {
                    missing.push(parent.to_string());
                }
            }
        }
    }
    let mut out = RecordMap::default();
    if missing.is_empty() {
        return out;
    }
    let elsewhere = InventoryOrigin {
        scope: InventoryScope::Project,
        id: None,
        name: "Elsewhere".to_string(),
    };
    for (_, found) in find_records_anywhere(mount, &missing) {
        out.set(tag(found.record, &elsewhere, Some(&found.mount_point_id)));
    }
    tracing::debug!(
        wanted = missing.len(),
        found = out.len(),
        context = "inventory",
        "[InventoryPool] Fetched archetypes outside the pool"
    );
    out
}

/// v4 `LoadInventoryPoolOptions`.
#[derive(Debug, Clone, Default)]
pub struct LoadInventoryPoolOptions<'a> {
    pub character_id: &'a str,
    /// Resolve the project tier from this chat.
    pub chat_id: Option<&'a str>,
    /// Or name the project directly (no chat).
    pub project_id: Option<&'a str>,
    /// The character's own records, already read.
    pub own_records: Option<&'a [InventoryRecord]>,
}

/// The project's display name for its tier's origin; `"Project"` when it
/// cannot be read (v4 `projectNameOf`; a thrown read WARNs `[InventoryPool]
/// Project name lookup failed`).
pub fn project_name_of(main: &Connection, mount: &Connection, project_id: Option<&str>) -> String {
    let Some(project_id) = project_id.filter(|s| !s.is_empty()) else {
        return "Project".to_string();
    };
    match ProjectsRepository::new(main, mount).find_by_id(project_id) {
        Ok(Some(p)) => p
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("Project")
            .to_string(),
        Ok(None) => "Project".to_string(),
        Err(error) => {
            tracing::warn!(
                projectId = %project_id,
                context = "inventory",
                error = %error,
                "[InventoryPool] Project name lookup failed"
            );
            "Project".to_string()
        }
    }
}

/// Load a character's Inventory pool (v4 `loadInventoryPool`): own vault,
/// groups, the project tier (from `chat_id` or `project_id`) and General.
/// Each tier fails soft. DEBUG `[InventoryPool] Loaded inventory pool` (its
/// `reachable` is the ARCHIVED-INCLUDED count — `byId.size`).
pub fn load_inventory_pool(
    main: &Connection,
    mount: &Connection,
    opts: LoadInventoryPoolOptions<'_>,
) -> Result<InventoryPool, DbError> {
    let character_id = opts.character_id;
    let mut project_id: Option<String> = opts
        .project_id
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if project_id.is_none() {
        if let Some(chat_id) = opts.chat_id.filter(|s| !s.is_empty()) {
            // `repos.chats.findById` is the fallback read — a failure logs
            // `Error finding entity by ID` and answers null, so v4's
            // `Project lookup for chat failed` catch is unreachable.
            project_id = chats_read::find_by_id_or_none(main, chat_id).and_then(|c| {
                c.get("projectId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        }
    }

    // `repos.characters.findByIdRaw` — the fallback `_findById`.
    let character = characters_read::find_by_id_raw_or_none(main, character_id);
    let own_mount_point_id: Option<String> = character
        .as_ref()
        .and_then(|c| {
            c.get("characterDocumentMountPointId")
                .and_then(Value::as_str)
        })
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let groups = resolve_group_mounts_for_character(main, mount, character_id);
    let project_mount_point_ids = resolve_project_mount_point_ids(mount, project_id.as_deref());
    let general_mount_point_id = instance_settings::get_general_mount_point_id(main)?;
    let project_name = project_name_of(main, mount, project_id.as_deref());

    let mut group_origin_by_mount: Vec<(String, InventoryOrigin)> = Vec::new();
    for g in &groups {
        for mp in &g.mount_point_ids {
            let origin = InventoryOrigin {
                scope: InventoryScope::Group,
                id: Some(g.group.id.clone()),
                name: g.group.name.clone(),
            };
            match group_origin_by_mount.iter_mut().find(|(k, _)| k == mp) {
                Some((_, slot)) => *slot = origin,
                None => group_origin_by_mount.push((mp.clone(), origin)),
            }
        }
    }
    let own_origin = InventoryOrigin {
        scope: InventoryScope::Character,
        id: Some(character_id.to_string()),
        name: character
            .as_ref()
            .and_then(|c| c.get("name").and_then(Value::as_str))
            .unwrap_or("")
            .to_string(),
    };
    let project_origin = InventoryOrigin {
        scope: InventoryScope::Project,
        id: project_id.clone(),
        name: project_name,
    };

    let mut unread: Vec<UnreadInventoryMount> = Vec::new();
    let mut consulted: Vec<String> = match (&opts.own_records, &own_mount_point_id) {
        (Some(_), Some(mp)) => vec![mp.clone()],
        _ => Vec::new(),
    };
    let own: Vec<InventoryRecord> = match (opts.own_records, &own_mount_point_id) {
        (Some(records), mp) => records
            .iter()
            .cloned()
            .map(|r| tag(r, &own_origin, mp.as_deref()))
            .collect(),
        (None, Some(mp)) => read_tier(
            mount,
            std::slice::from_ref(mp),
            |_| own_origin.clone(),
            &mut unread,
            &mut consulted,
        ),
        (None, None) => Vec::new(),
    };
    let group_mps: Vec<String> = group_origin_by_mount
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    let group = read_tier(
        mount,
        &group_mps,
        |mp| {
            group_origin_by_mount
                .iter()
                .find(|(k, _)| k == mp)
                .map(|(_, o)| o.clone())
                .expect("a group mount has an origin")
        },
        &mut unread,
        &mut consulted,
    );
    let project_mps: Vec<String> = project_mount_point_ids
        .iter()
        .filter(|mp| Some(mp.as_str()) != own_mount_point_id.as_deref())
        .cloned()
        .collect();
    let project = read_tier(
        mount,
        &project_mps,
        |_| project_origin.clone(),
        &mut unread,
        &mut consulted,
    );
    let general_mps: Vec<String> = general_mount_point_id.iter().cloned().collect();
    let general = read_tier(
        mount,
        &general_mps,
        |_| general_inventory_origin(),
        &mut unread,
        &mut consulted,
    );

    let external = fetch_external_parents(mount, &[&own, &group, &project, &general]);
    let (own_count, group_count, project_count, general_count) =
        (own.len(), group.len(), project.len(), general.len());
    let unread_count = unread.len();
    let pool = build_inventory_pool(
        Some(character_id.to_string()),
        InventoryTierMounts {
            group_mount_point_ids: group_mps,
            project_mount_point_ids,
        },
        InventoryLayers {
            own,
            group,
            project,
            general,
        },
        external,
        unread,
        consulted,
    );
    tracing::debug!(
        characterId = %character_id,
        chatId = opts.chat_id.unwrap_or("null"),
        projectId = project_id.as_deref().unwrap_or("null"),
        ownCount = own_count,
        groupCount = group_count,
        projectCount = project_count,
        generalCount = general_count,
        reachable = pool.by_id.len(),
        unread = unread_count,
        op = "pool",
        context = "inventory",
        "[InventoryPool] Loaded inventory pool"
    );
    Ok(pool)
}

/// v4 `loadPoolForLocation`'s context.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadPoolContext<'a> {
    pub chat_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
}

/// The pool a location writes against (v4 `loadPoolForLocation`): a
/// character's whole reach, or for a shared tier its own folder plus Quilltap
/// General (a project or group store reaches General; General reaches only
/// itself).
pub fn load_pool_for_location(
    main: &Connection,
    mount: &Connection,
    location: &InventoryLocation,
    ctx: LoadPoolContext<'_>,
) -> Result<InventoryPool, DbError> {
    if let Some(character_id) = location.character_id.as_deref() {
        return load_inventory_pool(
            main,
            mount,
            LoadInventoryPoolOptions {
                character_id,
                chat_id: ctx.chat_id,
                project_id: ctx.project_id,
                own_records: None,
            },
        );
    }
    let own: Vec<InventoryRecord> = location
        .read_items(mount, true)?
        .into_iter()
        .map(|r| tag(r, &location.origin, Some(&location.mount_point_id)))
        .collect();
    let general_mount_point_id = if location.scope == InventoryScope::General {
        None
    } else {
        instance_settings::get_general_mount_point_id(main)?
    };
    let mut unread = Vec::new();
    let mut consulted = vec![location.mount_point_id.clone()];
    let general_mps: Vec<String> = general_mount_point_id.into_iter().collect();
    let general = read_tier(
        mount,
        &general_mps,
        |_| general_inventory_origin(),
        &mut unread,
        &mut consulted,
    );
    let external = fetch_external_parents(mount, &[&own, &general]);
    Ok(build_inventory_pool(
        None,
        InventoryTierMounts::default(),
        InventoryLayers {
            own,
            group: Vec::new(),
            project: Vec::new(),
            general,
        },
        external,
        unread,
        consulted,
    ))
}

/// One pool per cast member for a chat (v4 `loadCastInventoryPools`), in cast
/// order.
pub fn load_cast_inventory_pools(
    main: &Connection,
    mount: &Connection,
    chat_id: &str,
    project_id: Option<&str>,
    character_ids: &[String],
) -> Result<Vec<(String, InventoryPool)>, DbError> {
    let mut out: Vec<(String, InventoryPool)> = Vec::new();
    for character_id in character_ids {
        let pool = load_inventory_pool(
            main,
            mount,
            LoadInventoryPoolOptions {
                character_id,
                chat_id: Some(chat_id),
                project_id,
                own_records: None,
            },
        )?;
        // `new Map(ids.map(...))` — a repeated id keeps its first position,
        // the later pool.
        match out.iter_mut().find(|(k, _)| k == character_id) {
            Some((_, slot)) => *slot = pool,
            None => out.push((character_id.clone(), pool)),
        }
    }
    Ok(out)
}

/// Reachable, equippable kin of `name` in the pool, resolved (v4 `kinOf`).
pub fn kin_of(name: &str, pool: &InventoryPool) -> Vec<ResolvedInventoryItem> {
    let probe = InventoryRecord(
        [("name".to_string(), Value::String(name.to_string()))]
            .into_iter()
            .collect(),
    );
    pool.resolved()
        .into_iter()
        .filter(|item| {
            !item.is_archetype() && item.kind() != InventoryKind::Kit && is_kin(item, &probe)
        })
        .collect()
}
