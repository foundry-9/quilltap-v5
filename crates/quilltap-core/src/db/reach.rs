//! Reach — what a character, or a cast, can reach, settled once (v4
//! `lib/mount-index/reach.ts`, NEW in `0c47f4539`, "Refactor 4.12, tier walk").
//!
//! v4's header, carried: Quilltap has one idea about stores — a character
//! reaches four tiers, **character vault > group stores > project stores >
//! Quilltap General**, nearer shadowing farther.
//! [`resolve_tiered_mount_pool`](super::tiered_mount_pool::resolve_tiered_mount_pool)
//! is the forward walk for one character; this module is everything around it
//! that used to be copied at call sites:
//!
//! - [`resolve_owner_store`] — `{ scope, id }` to the owner's official store
//!   (the three `resolveXxxLocation` chokepoints are wrappers over it). Only a
//!   writer passes `ensure`; a read never provisions.
//! - [`group_mounts_for`] / [`group_origin_map`] — the group tier kept
//!   grouped, in membership order, and the "which group is this mount" origin
//!   map.
//! - [`resolve_cast_pool`] / [`cast_group_mounts`] — a cast's reach (the
//!   Scenario Builder, the operator's document picker, the cast's scenario
//!   menu): the union over the live cast.
//! - [`order_mounts`] / [`bucket_mounts`] — a pool as an ordered probe list,
//!   or as knowledge tiers with their literal-phrase boosts.
//! - [`classify_mount`] — the reverse question: whose store is this mount?
//!
//! ## The settled rules (v4's tier walk §5.1)
//!
//! - The project tier is every linked store, the official store first.
//! - A character's group tier is the character's own groups; a cast's is the
//!   union over the live cast.
//! - Inside a tier, the earlier mount wins: membership order, and the first
//!   group to claim a shared store keeps it. Readers that shadow weakest-first
//!   reverse [`order_mounts`]' order; nobody sorts by id.
//! - The project roster and a disabled store are named options on the forward
//!   walk (`roster`, `enabled`), never applied around it.
//! - An archived character's vault still reads (its items stay visible) and
//!   refuses writes; scenarios refuse it outright, by option.
//!
//! ## The reads, measured (P4.D376)
//!
//! Every owner read is RAW, as v4's (ruling R-F): `characters.findByIdRaw`,
//! `projects.findByIdRaw`, `groups.findByIdRaw`, and the three
//! `findBy…MountPointIdRaw` finders. All of them are fallback-mode
//! `safeQuery`s in v4 — a failed or invalid row logs the REPOSITORY lines and
//! answers `null` — so v5 reads through the `*_raw_or_none` twins, which log
//! the same lines through [`super::fallback`].
//!
//! v4's two WARNs in [`resolve_cast_pool`] — `[Reach] Cast vault lookup failed;
//! tier dropped for this character` and `[Reach] Quilltap General lookup
//! failed; global tier dropped` — are UNREACHABLE through v4's real code: the
//! first sits behind `findByIdRaw` (a fallback `safeQuery` that never throws),
//! the second behind `getGeneralMountPointId` → `readSetting`, which catches its
//! own failure. v5 keeps the General arm's shape (its reader is `Result`-typed;
//! `read_setting` also swallows since P4.113) and takes the cast read's
//! fallback in place of its WARN. Both are pinned NEGATIVE in
//! `reach_equivalence`.
//!
//! Server-only.

use rusqlite::Connection;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use super::doc_mount_file_links::DocMountFileLinksRepository;
use super::ensure_official_store::{ensure_owner_official_store, StoreOwnerKind};
use super::tiered_mount_pool::{
    dedupe_tier_triple, resolve_group_mounts_for_character_with, resolve_mount_point_ids_for_group,
    resolve_project_mount_point_ids, GroupMounts, GroupMountsOptions, MountTier, TierTriple,
    TieredMountPool,
};
use super::{characters_read, groups, instance_settings, projects, DbError};
use crate::literal_boost::{
    LITERAL_BOOST_CHARACTER, LITERAL_BOOST_GLOBAL, LITERAL_BOOST_GROUP, LITERAL_BOOST_PROJECT,
};

/// The owner name v4 gives Quilltap General.
pub const GENERAL_OWNER_NAME: &str = "Quilltap General";

/// JS truthiness of a JSON cell (`null`, `false`, `0`, `""` are falsy).
fn js_truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// A truthy string cell, or `None`.
fn truthy_str(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty())
}

// ============================================================================
// Owner → official store
// ============================================================================

/// The four owners a store can belong to (v4 `OwnerScope`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OwnerScope {
    Character,
    Group,
    Project,
    General,
}

impl OwnerScope {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            OwnerScope::Character => "character",
            OwnerScope::Group => "group",
            OwnerScope::Project => "project",
            OwnerScope::General => "general",
        }
    }
}

/// What a folder that cannot be made does (v4 `onFolderFailure`): logged and
/// swallowed (the write re-creates it), or rethrown (the caller must see it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerFolderFailure {
    Swallow,
    Throw,
}

/// v4 `ResolveOwnerStoreOptions.ensure`: provision a missing project / group
/// store and heal `folder` inside it (for General, only the folder). For
/// writers only — a read probe omits it and gets `None` for a store that does
/// not exist yet.
#[derive(Debug, Clone, Copy)]
pub struct EnsureFolder<'a> {
    pub folder: &'a str,
    pub on_folder_failure: OwnerFolderFailure,
}

/// v4 `ResolveOwnerStoreOptions.archived` — REQUIRED (no default): `Allow`, an
/// archived character's vault resolves (it reads; writes refuse at the
/// writer); `Refuse`, an archived character resolves to `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archived {
    Allow,
    Refuse,
}

/// v4 `ResolveOwnerStoreOptions`. Deliberately no `Default`: `archived` is
/// required, as v4's type makes it.
#[derive(Debug, Clone, Copy)]
pub struct ResolveOwnerStoreOptions<'a> {
    pub ensure: Option<EnsureFolder<'a>>,
    pub archived: Archived,
    /// A tag for the log lines, e.g. `"wardrobe"`; OMITTED from them when
    /// `None` (v4's `undefined`).
    pub context: Option<&'a str>,
}

/// The owner half of an [`OwnerStore`]: a character, project or group; `id:
/// None` for General.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OwnerRef {
    pub id: Option<String>,
    pub name: String,
}

/// An owner's store, as [`resolve_owner_store`] found it (v4 `OwnerStore`).
/// Serialises in v4's key order, `character` omitted when absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerStore {
    pub scope: OwnerScope,
    pub mount_point_id: String,
    pub owner: OwnerRef,
    /// The RAW character row (`characters_read::find_by_id_raw` — no vault
    /// overlay), for the character scope. v4 types it as the
    /// `ReachCharacterRow` slice but hands back the whole raw row; so does v5.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<Value>,
}

/// v4's private `ensureOwnerFolder`: heal `ensure.folder`; on a failure WARN
/// `[Reach] Failed to ensure ${folder} folder {mountPointId, folder, context,
/// error}` (`context` omitted when absent, `error` the BARE message), then
/// rethrow only under [`OwnerFolderFailure::Throw`].
fn ensure_owner_folder(
    mount: &Connection,
    mount_point_id: &str,
    ensure: &EnsureFolder<'_>,
    context: Option<&str>,
) -> Result<(), DbError> {
    if let Err(error) =
        DocMountFileLinksRepository::new(mount).ensure_folder_path(mount_point_id, ensure.folder)
    {
        let message = format!("[Reach] Failed to ensure {} folder", ensure.folder);
        tracing::warn!(
            mountPointId = %mount_point_id,
            folder = %ensure.folder,
            context = context.map(tracing::field::display),
            error = %super::fallback::error_text(&error),
            "{message}"
        );
        if ensure.on_folder_failure == OwnerFolderFailure::Throw {
            return Err(error);
        }
    }
    Ok(())
}

/// v4 `resolveOwnerStore(scope, id, repos, userId, opts)`: `{ scope, id }` to
/// the owner's store, or `None` when the owner is missing (or, for a
/// character, not `user_id`'s, or archived under [`Archived::Refuse`]), when a
/// non-General scope arrives without an id, or when the store does not exist
/// and `ensure` is off (or cannot be made).
///
/// v4's *why*: reads are raw — only the owner's name and store pointer are
/// wanted, and a broken vault overlay should cost a `None` rather than a
/// throw. `ensure` never provisions a CHARACTER's vault (it is not ensured
/// here). The answered owner name is the RAW `name`, not the `'Project'` /
/// `'Group'` fallback the ensure arm names a minted store with.
///
/// `ensure` writes (it may mint a store and a folder), so a caller passing it
/// holds both connections writable.
pub fn resolve_owner_store(
    main: &Connection,
    mount: &Connection,
    scope: OwnerScope,
    id: Option<&str>,
    user_id: &str,
    opts: &ResolveOwnerStoreOptions<'_>,
) -> Result<Option<OwnerStore>, DbError> {
    let context = opts.context;

    if scope == OwnerScope::General {
        let Some(mount_point_id) =
            instance_settings::get_general_mount_point_id(main)?.filter(|m| !m.is_empty())
        else {
            return Ok(None);
        };
        if let Some(ensure) = &opts.ensure {
            ensure_owner_folder(mount, &mount_point_id, ensure, context)?;
        }
        return Ok(Some(OwnerStore {
            scope,
            mount_point_id,
            owner: OwnerRef {
                id: None,
                name: GENERAL_OWNER_NAME.to_string(),
            },
            character: None,
        }));
    }

    // JS truthiness: '' is no id.
    let Some(id) = id.filter(|i| !i.is_empty()) else {
        return Ok(None);
    };

    if scope == OwnerScope::Character {
        let Some(character) = characters_read::find_by_id_raw_or_none(main, id) else {
            return Ok(None);
        };
        // `character.userId !== userId` — strict: an absent owner is refused.
        if character.get("userId").and_then(Value::as_str) != Some(user_id) {
            return Ok(None);
        }
        if opts.archived == Archived::Refuse && js_truthy(character.get("archivedAt")) {
            tracing::debug!(
                characterId = %id,
                context = context.map(tracing::field::display),
                "[Reach] Archived character refused"
            );
            return Ok(None);
        }
        let Some(mount_point_id) =
            truthy_str(character.get("characterDocumentMountPointId")).map(str::to_string)
        else {
            return Ok(None);
        };
        let owner = OwnerRef {
            id: character
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string),
            name: character
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
        return Ok(Some(OwnerStore {
            scope,
            mount_point_id,
            owner,
            character: Some(character),
        }));
    }

    let (owner, kind, fallback) = if scope == OwnerScope::Project {
        (
            projects::find_by_id_raw_or_none(main, id),
            StoreOwnerKind::Project,
            "Project",
        )
    } else {
        (
            groups::find_by_id_raw_or_none(main, id),
            StoreOwnerKind::Group,
            "Group",
        )
    };
    let Some(owner) = owner else {
        return Ok(None);
    };
    let owner_id = str_of(&owner, "id").unwrap_or(id).to_string();
    let owner_name = str_of(&owner, "name").unwrap_or_default().to_string();

    let mount_point_id = if let Some(ensure) = &opts.ensure {
        let fallback_name = if owner_name.is_empty() {
            fallback
        } else {
            owner_name.as_str()
        };
        let Some(ensured) =
            ensure_owner_official_store(main, mount, kind, &owner_id, fallback_name)?
        else {
            return Ok(None);
        };
        ensure_owner_folder(mount, &ensured.mount_point_id, ensure, context)?;
        Some(ensured.mount_point_id)
    } else {
        str_of(&owner, "officialMountPointId").map(str::to_string)
    };
    let Some(mount_point_id) = mount_point_id.filter(|m| !m.is_empty()) else {
        return Ok(None);
    };
    Ok(Some(OwnerStore {
        scope,
        mount_point_id,
        owner: OwnerRef {
            id: Some(owner_id),
            name: owner_name,
        },
        character: None,
    }))
}

fn str_of<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

// ============================================================================
// Group tier, kept grouped
// ============================================================================

/// v4 `groupMountsFor` — an alias of `resolveGroupMountsForCharacter`: a
/// character's group tier, one entry per group in membership order, each with
/// its official store first; a store shared by two groups is credited to the
/// first. `include_empty` keeps groups with no stores (a listing of the
/// character's groups, not a walk).
pub fn group_mounts_for(
    main: &Connection,
    mount: &Connection,
    character_id: &str,
    opts: GroupMountsOptions,
) -> Vec<GroupMounts> {
    resolve_group_mounts_for_character_with(main, mount, character_id, opts)
}

/// A group-tier origin, in the shape the wardrobe and inventory both tag with
/// (v4 `GroupOrigin`). Serialises `{ scope: "group", id, name }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupOrigin {
    pub id: String,
    pub name: String,
}

impl Serialize for GroupOrigin {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(3))?;
        m.serialize_entry("scope", "group")?;
        m.serialize_entry("id", &self.id)?;
        m.serialize_entry("name", &self.name)?;
        m.end()
    }
}

/// v4 `groupOriginMap(groups)`: mount → the group it hangs in, for tagging a
/// group-tier read. Keeps the tier's order; the first claim is kept.
pub fn group_origin_map(groups: &[GroupMounts]) -> indexmap::IndexMap<String, GroupOrigin> {
    let mut out: indexmap::IndexMap<String, GroupOrigin> = indexmap::IndexMap::new();
    for g in groups {
        for mount_point_id in &g.mount_point_ids {
            if !out.contains_key(mount_point_id) {
                out.insert(
                    mount_point_id.clone(),
                    GroupOrigin {
                        id: g.group.id.clone(),
                        name: g.group.name.clone(),
                    },
                );
            }
        }
    }
    out
}

// ============================================================================
// A cast's reach
// ============================================================================

/// `[...new Set(ids.filter(Boolean))]` — insertion-ordered, empty ids dropped.
fn unique_truthy(ids: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids.iter().filter(|id| !id.is_empty()) {
        if !out.contains(id) {
            out.push(id.clone());
        }
    }
    out
}

/// v4 `castGroupMounts(characterIds, groupIds = [])`: the group stores a cast
/// reaches — the groups named outright (in the order given), then each
/// character's groups in cast order, deduped, the first claim kept. The caller
/// decides who is in the cast. Fails soft per group.
pub fn cast_group_mounts(
    main: &Connection,
    mount: &Connection,
    character_ids: &[String],
    group_ids: &[String],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |ids: Vec<String>| {
        for id in ids {
            if !out.contains(&id) {
                out.push(id);
            }
        }
    };
    for group_id in unique_truthy(group_ids) {
        add(resolve_mount_point_ids_for_group(main, mount, &group_id));
    }
    for character_id in unique_truthy(character_ids) {
        add(
            group_mounts_for(main, mount, &character_id, GroupMountsOptions::default())
                .into_iter()
                .flat_map(|g| g.mount_point_ids)
                .collect(),
        );
    }
    out
}

/// One live cast member's vault (v4 `CastPool.castVaults[]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CastVault {
    pub character_id: String,
    pub mount_point_id: String,
}

/// A cast's pool (v4 `CastPool extends TieredMountPool`): the cast's vaults in
/// the participant tier, and who owns each. Serialises in v4's key order —
/// the spread of the deduped scoped tiers, then `participantMountPointIds`,
/// `castVaults`, `liveCharacterIds`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CastPool {
    pub pool: TieredMountPool,
    /// The live cast members' vaults, in cast order (a vault deduped into a
    /// scoped tier is absent).
    pub cast_vaults: Vec<CastVault>,
    /// The cast members that count: the caller's, not archived.
    pub live_character_ids: Vec<String>,
}

impl Serialize for CastPool {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = &self.pool;
        let mut m = s.serialize_map(Some(7))?;
        m.serialize_entry("characterMountPointId", &p.character_mount_point_id)?;
        m.serialize_entry("groupMountPointIds", &p.group_mount_point_ids)?;
        m.serialize_entry("projectMountPointIds", &p.project_mount_point_ids)?;
        m.serialize_entry("globalMountPointId", &p.global_mount_point_id)?;
        m.serialize_entry("participantMountPointIds", &p.participant_mount_point_ids)?;
        m.serialize_entry("castVaults", &self.cast_vaults)?;
        m.serialize_entry("liveCharacterIds", &self.live_character_ids)?;
        m.end()
    }
}

/// v4 `resolveCastPool`'s options bag.
#[derive(Debug, Clone, Copy)]
pub struct CastPoolOptions<'a> {
    pub user_id: &'a str,
    pub project_id: Option<&'a str>,
    pub character_ids: &'a [String],
    /// Groups named outright (a run launched from a group's page).
    pub group_ids: &'a [String],
}

/// v4 `resolveCastPool(opts)`: what a cast can reach, before or outside any
/// one character's turn — every live cast member's vault (participant tier),
/// the groups named outright plus the union of the cast's groups, the
/// project's stores and Quilltap General. There is no acting character, so
/// `character_mount_point_id` is `None`; flatten with `include_participants`.
///
/// v4's *why*: an archived character is a tombstone and contributes nothing
/// (never `ensureCharacterVault` one here). Any tier whose lookup fails drops
/// out; this never fails.
///
/// The DEBUG `[Reach] Resolved cast pool` counts `namedGroupCount` over the
/// RAW `groupIds` (v4's `opts.groupIds?.length ?? 0` — empty strings and
/// duplicates counted), the other counts after the dedup.
pub fn resolve_cast_pool(
    main: &Connection,
    mount: &Connection,
    opts: &CastPoolOptions<'_>,
) -> CastPool {
    let cast_ids = unique_truthy(opts.character_ids);

    // 1. Live cast vaults. Raw reads: only the vault pointer, owner and
    //    archive flag. v4's `findByIdRaw` is a fallback `safeQuery`, so a
    //    failed read logs the repository line and skips the member — the
    //    `Cast vault lookup failed` WARN is unreachable (module doc).
    let mut live_character_ids: Vec<String> = Vec::new();
    let mut vaults: Vec<CastVault> = Vec::new();
    for character_id in &cast_ids {
        let Some(character) = characters_read::find_by_id_raw_or_none(main, character_id) else {
            continue;
        };
        // `character.userId && character.userId !== opts.userId` — a userless
        // row is ADMITTED.
        if let Some(owner) = truthy_str(character.get("userId")) {
            if owner != opts.user_id {
                continue;
            }
        }
        if js_truthy(character.get("archivedAt")) {
            tracing::debug!(
                characterId = %character_id,
                "[Reach] Archived cast member contributes nothing to the pool"
            );
            continue;
        }
        live_character_ids.push(character_id.clone());
        if let Some(vault) = truthy_str(character.get("characterDocumentMountPointId")) {
            vaults.push(CastVault {
                character_id: character_id.clone(),
                mount_point_id: vault.to_string(),
            });
        }
    }

    // 2–4. Group, project and General tiers.
    let group_mount_point_ids = cast_group_mounts(main, mount, &live_character_ids, opts.group_ids);
    let project_mount_point_ids = resolve_project_mount_point_ids(main, mount, opts.project_id);
    let global_mount_point_id = match instance_settings::get_general_mount_point_id(main) {
        Ok(g) => g,
        Err(e) => {
            // Unreachable through v4's real code (module doc); kept for the
            // `Result`-typed reader.
            tracing::warn!(
                error = %super::fallback::error_text(&e),
                "[Reach] Quilltap General lookup failed; global tier dropped"
            );
            None
        }
    };

    // 5. Scoped-tier dedup, then the cast vaults excluded from every scoped
    //    tier so each mount classifies into exactly one bucket.
    let deduped = dedupe_tier_triple(TierTriple {
        character_mount_point_id: None,
        group_mount_point_ids,
        project_mount_point_ids,
        global_mount_point_id,
    });
    let excluded = |id: &str| {
        deduped.group_mount_point_ids.iter().any(|g| g == id)
            || deduped.project_mount_point_ids.iter().any(|p| p == id)
            || deduped.global_mount_point_id.as_deref() == Some(id)
    };
    let mut seen: Vec<String> = Vec::new();
    let cast_vaults: Vec<CastVault> = vaults
        .into_iter()
        .filter(|v| {
            if excluded(&v.mount_point_id) || seen.contains(&v.mount_point_id) {
                return false;
            }
            seen.push(v.mount_point_id.clone());
            true
        })
        .collect();

    let pool = TieredMountPool {
        character_mount_point_id: deduped.character_mount_point_id,
        participant_mount_point_ids: cast_vaults
            .iter()
            .map(|v| v.mount_point_id.clone())
            .collect(),
        group_mount_point_ids: deduped.group_mount_point_ids,
        project_mount_point_ids: deduped.project_mount_point_ids,
        global_mount_point_id: deduped.global_mount_point_id,
    };
    tracing::debug!(
        castCount = cast_ids.len(),
        liveCastCount = live_character_ids.len(),
        namedGroupCount = opts.group_ids.len(),
        participants = pool.participant_mount_point_ids.len(),
        groups = pool.group_mount_point_ids.len(),
        projects = pool.project_mount_point_ids.len(),
        hasGlobal = pool.global_mount_point_id.is_some(),
        "[Reach] Resolved cast pool"
    );
    CastPool {
        pool,
        cast_vaults,
        live_character_ids,
    }
}

// ============================================================================
// A pool, ordered
// ============================================================================

/// The tiers in precedence order, nearest first (v4 `TIER_ORDER`).
pub const TIER_ORDER: [MountTier; 5] = [
    MountTier::Character,
    MountTier::Participant,
    MountTier::Group,
    MountTier::Project,
    MountTier::Global,
];

/// One probe of an ordered pool (v4 `{ tier, mountPointId }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedMount {
    pub tier: MountTier,
    pub mount_point_id: String,
}

/// v4 `orderMounts(pool, { includeParticipants })`: a pool as a probe list,
/// strongest first — tier by tier, and inside a tier in the pool's own order
/// (membership order for groups, the official store first for the project).
/// The first probe that answers wins; a reader that shadows weakest-first
/// reverses this. Participants are left out unless asked for. One `seen` dedup
/// across tiers; NO sort.
pub fn order_mounts(pool: &TieredMountPool, include_participants: bool) -> Vec<OrderedMount> {
    let mut seen: Vec<&str> = Vec::new();
    let mut ordered: Vec<OrderedMount> = Vec::new();
    for tier in TIER_ORDER {
        let ids: Vec<&str> = match tier {
            MountTier::Character => pool
                .character_mount_point_id
                .as_deref()
                .into_iter()
                .collect(),
            MountTier::Participant if include_participants => pool
                .participant_mount_point_ids
                .iter()
                .map(String::as_str)
                .collect(),
            MountTier::Participant => Vec::new(),
            MountTier::Group => pool
                .group_mount_point_ids
                .iter()
                .map(String::as_str)
                .collect(),
            MountTier::Project => pool
                .project_mount_point_ids
                .iter()
                .map(String::as_str)
                .collect(),
            MountTier::Global => pool.global_mount_point_id.as_deref().into_iter().collect(),
        };
        for id in ids {
            // v4's `pool.characterMountPointId ? [...] : []` — an empty id is
            // falsy only on the two singleton tiers; the lists keep it.
            if matches!(tier, MountTier::Character | MountTier::Global) && id.is_empty() {
                continue;
            }
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            ordered.push(OrderedMount {
                tier,
                mount_point_id: id.to_string(),
            });
        }
    }
    ordered
}

/// A knowledge tier's name (v4 `KnowledgeBucket.tier`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeTier {
    Character,
    Group,
    Project,
    Global,
}

/// A knowledge tier: the mounts searched together and the literal-phrase boost
/// they earn (v4 `KnowledgeBucket`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeBucket {
    pub tier: KnowledgeTier,
    pub mount_point_ids: Vec<String>,
    pub boost: f64,
}

/// v4 `bucketMounts`'s `scope` (default `'all'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BucketScope {
    #[default]
    All,
    Character,
    Group,
    Project,
}

/// v4 `bucketMounts(pool, { scope, participantsAsCharacter })`: a pool as the
/// knowledge searches' tiers, nearest first, each with its literal-phrase
/// boost so a hit in a closer voice outranks the same hit further out. `scope`
/// narrows as the scriptorium search does; `participants_as_character` puts a
/// cast pool's vaults where the character tier would be (there is no acting
/// character). Empty tiers are omitted; the input is trusted to be deduped
/// already (the pool's job), so nothing is deduped here.
pub fn bucket_mounts(
    pool: &TieredMountPool,
    scope: BucketScope,
    participants_as_character: bool,
) -> Vec<KnowledgeBucket> {
    let want = |tier: BucketScope| scope == BucketScope::All || scope == tier;
    let mut buckets: Vec<KnowledgeBucket> = Vec::new();
    if want(BucketScope::Character) {
        if let Some(c) = pool
            .character_mount_point_id
            .as_deref()
            .filter(|c| !c.is_empty())
        {
            buckets.push(KnowledgeBucket {
                tier: KnowledgeTier::Character,
                mount_point_ids: vec![c.to_string()],
                boost: LITERAL_BOOST_CHARACTER,
            });
        }
    }
    if want(BucketScope::Character)
        && participants_as_character
        && !pool.participant_mount_point_ids.is_empty()
    {
        buckets.push(KnowledgeBucket {
            tier: KnowledgeTier::Character,
            mount_point_ids: pool.participant_mount_point_ids.clone(),
            boost: LITERAL_BOOST_CHARACTER,
        });
    }
    if want(BucketScope::Group) && !pool.group_mount_point_ids.is_empty() {
        buckets.push(KnowledgeBucket {
            tier: KnowledgeTier::Group,
            mount_point_ids: pool.group_mount_point_ids.clone(),
            boost: LITERAL_BOOST_GROUP,
        });
    }
    if want(BucketScope::Project) && !pool.project_mount_point_ids.is_empty() {
        buckets.push(KnowledgeBucket {
            tier: KnowledgeTier::Project,
            mount_point_ids: pool.project_mount_point_ids.clone(),
            boost: LITERAL_BOOST_PROJECT,
        });
    }
    if scope == BucketScope::All {
        if let Some(g) = pool
            .global_mount_point_id
            .as_deref()
            .filter(|g| !g.is_empty())
        {
            buckets.push(KnowledgeBucket {
                tier: KnowledgeTier::Global,
                mount_point_ids: vec![g.to_string()],
                boost: LITERAL_BOOST_GLOBAL,
            });
        }
    }
    buckets
}

// ============================================================================
// Mount → owner
// ============================================================================

/// Whose store a mount is (v4 `MountOwnerInfo`). Serialises in v4's key
/// order, `character` omitted when absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MountOwnerInfo {
    pub scope: OwnerScope,
    /// The character, project or group id; `None` for General.
    pub owner_id: Option<String>,
    pub owner_name: String,
    /// True only for an archived character's vault.
    pub archived: bool,
    /// The raw character row, for a vault.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character: Option<Value>,
}

/// v4 `classifyMount(mountPointId, repos)`: the reverse question — which
/// owner's store is this mount: General, a character's vault, or a project's
/// or group's *official* store — or `None` for any other store (a LINKED store
/// belongs to no one owner). Raw reads, in v4's order; the caller filters
/// (another user's or an archived character, say).
pub fn classify_mount(
    main: &Connection,
    mount_point_id: &str,
) -> Result<Option<MountOwnerInfo>, DbError> {
    if instance_settings::get_general_mount_point_id(main)?.as_deref() == Some(mount_point_id) {
        return Ok(Some(MountOwnerInfo {
            scope: OwnerScope::General,
            owner_id: None,
            owner_name: GENERAL_OWNER_NAME.to_string(),
            archived: false,
            character: None,
        }));
    }
    if let Some(character) =
        characters_read::find_by_document_mount_point_id_raw(main, mount_point_id)
    {
        let text = |k: &str| character.get(k).and_then(Value::as_str).map(str::to_string);
        return Ok(Some(MountOwnerInfo {
            scope: OwnerScope::Character,
            owner_id: text("id"),
            owner_name: text("name").unwrap_or_default(),
            archived: js_truthy(character.get("archivedAt")),
            character: Some(character),
        }));
    }
    if let Some(project) = projects::find_by_official_mount_point_id_raw(main, mount_point_id) {
        return Ok(Some(MountOwnerInfo {
            scope: OwnerScope::Project,
            owner_id: str_of(&project, "id").map(str::to_string),
            owner_name: str_of(&project, "name").unwrap_or_default().to_string(),
            archived: false,
            character: None,
        }));
    }
    if let Some(group) = groups::find_by_official_mount_point_id_raw(main, mount_point_id) {
        return Ok(Some(MountOwnerInfo {
            scope: OwnerScope::Group,
            owner_id: str_of(&group, "id").map(str::to_string),
            owner_name: str_of(&group, "name").unwrap_or_default().to_string(),
            archived: false,
            character: None,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> TieredMountPool {
        TieredMountPool {
            character_mount_point_id: Some("c".into()),
            participant_mount_point_ids: vec!["p1".into(), "c".into()],
            group_mount_point_ids: vec!["g2".into(), "g1".into()],
            project_mount_point_ids: vec!["po".into(), "pl".into()],
            global_mount_point_id: Some("gen".into()),
        }
    }

    #[test]
    fn order_mounts_keeps_pool_order_and_dedups_across_tiers() {
        let got: Vec<String> = order_mounts(&pool(), true)
            .into_iter()
            .map(|m| m.mount_point_id)
            .collect();
        assert_eq!(got, ["c", "p1", "g2", "g1", "po", "pl", "gen"]);
        let without: Vec<String> = order_mounts(&pool(), false)
            .into_iter()
            .map(|m| m.mount_point_id)
            .collect();
        assert_eq!(without, ["c", "g2", "g1", "po", "pl", "gen"]);
    }

    #[test]
    fn cast_pool_serialises_in_v4_key_order() {
        let text = serde_json::to_string(&CastPool::default()).unwrap();
        assert_eq!(
            text,
            r#"{"characterMountPointId":null,"groupMountPointIds":[],"projectMountPointIds":[],"globalMountPointId":null,"participantMountPointIds":[],"castVaults":[],"liveCharacterIds":[]}"#
        );
    }

    #[test]
    fn the_folder_warn_omits_an_absent_context() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let ensure = EnsureFolder {
            folder: "Scenarios",
            on_folder_failure: OwnerFolderFailure::Swallow,
        };
        let (res, lines) = crate::test_support::captured_with(|| {
            ensure_owner_folder(&mount, "m-1", &ensure, None)
        });
        assert!(res.is_ok(), "swallow answers Ok");
        let line = lines
            .iter()
            .find(|l| l.contains("[Reach] Failed to ensure Scenarios folder"))
            .unwrap_or_else(|| panic!("{lines:?}"));
        assert!(
            line.contains("mountPointId=m-1 folder=Scenarios error="),
            "{line}"
        );
        assert!(!line.contains("context="), "{line}");
        drop(main);
        let throw = EnsureFolder {
            on_folder_failure: OwnerFolderFailure::Throw,
            ..ensure
        };
        let (res, lines) = crate::test_support::captured_with(|| {
            ensure_owner_folder(&mount, "m-1", &throw, Some("scenarios"))
        });
        assert!(res.is_err(), "throw rethrows");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("folder=Scenarios context=scenarios error=")),
            "{lines:?}"
        );
    }
}
