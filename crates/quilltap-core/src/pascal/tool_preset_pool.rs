//! Custom-tool run presets — where they are found (v4
//! `lib/pascal/tool-preset-pool.ts`, NEW in `fd165fce0`, re-optioned by
//! `0c47f4539`).
//!
//! A preset (`Tools/{toolname}.{preset}.settings.json`, naming contract in
//! [`super::tool_presets`]) is found by the same walk that finds definitions:
//! the invoker's own vault, then their groups' stores, then the project's, then
//! Quilltap General, nearest first. A nearer preset of the same name shadows a
//! farther one, exactly as a nearer `.tool.json` shadows a farther definition.
//! A personal "aggressive" therefore overrides the table's shared one without
//! touching it.
//!
//! The walk also names every store in that order as a place a preset may be
//! filed, so the run dialog can offer a save destination without re-deriving
//! the tiers on the client.
//!
//! Fresh on every call, for the same reason the roster is (see the freshness
//! note atop [`super::roster`]): the files live in stores the user edits.
//!
//! # The walk is a seam
//!
//! [`resolve_tool_preset_pool_from`] takes an already-resolved pool plus the two
//! reads v4's own `tool-preset-pool.test.ts` mocks (the mount row and the
//! `Tools/` listing), so that suite's mocks are the differential's spine (the
//! [`super::roster::resolve_roster_from_pool`] precedent).
//! [`resolve_tool_preset_pool`] wires the same walk to the live DB.
//!
//! # "Never throws" is not quite true
//!
//! v4's docblock says the walk never throws, and a store that will not LIST is
//! indeed reported in `unread`. But the mount-row read (`docMountPoints.findById`)
//! sits outside the `try`, so a failing read propagates out of the walk and out
//! of the route. v5 carries that: a `find_mount` error is the walk's `Err`.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::collation::locale_compare;
use crate::db::doc_mount_points::{DocMountPointsRepository, MountServiceInfo};
use crate::db::tiered_mount_pool::{
    resolve_tiered_mount_pool, Enabled, MountTier, Roster, TierContext, TierResolveOptions,
    TieredMountPool,
};
use crate::db::DbError;

use super::roster::{list_tools_folder_files, ordered_mounts};
use super::tool_presets::parse_preset_from_path;

/// v4 `CONTEXT`.
const CONTEXT: &str = "pascal.tool-preset-pool";

/// The pool options the preset walk resolves with (v4 `{ roster: 'open',
/// enabled: 'require' }`): the same pool the roster walks, but the POOL drops
/// disabled stores, so a disabled store is neither read nor offered as a save
/// destination. Stated explicitly — there is no default.
pub const PRESET_POOL_OPTIONS: (Roster, Enabled) = (Roster::Open, Enabled::Require);

/// A store a preset may be read from or filed in (v4 `ToolPresetStore`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPresetStore {
    pub tier: MountTier,
    pub mount_point_id: String,
    pub mount_name: String,
}

/// One preset that won its name, with the store it came from (v4 `ToolPreset`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPreset {
    pub store: ToolPresetStore,
    /// The preset name — the `{preset}` segment of the filename.
    pub name: String,
    /// Store-relative path of the file in its store.
    pub path: String,
    /// Farther stores holding a preset of the same name, nearest first.
    pub shadows: Vec<ToolPresetStore>,
}

/// A store whose `Tools/` folder could not be listed (v4 `ToolPresetStore & {
/// reason }`). Its presets are missing, not absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadPresetStore {
    pub store: ToolPresetStore,
    /// v4 `getErrorMessage(error)` — the BARE message (contrast the roster's
    /// `listing failed: <msg>`).
    pub reason: String,
}

/// v4 `ToolPresetPool`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolPresetPool {
    /// The winning preset per name, sorted by name (`localeCompare`).
    pub presets: Vec<ToolPreset>,
    /// Every enabled store on the walk, nearest first — the save destinations.
    pub stores: Vec<ToolPresetStore>,
    /// Stores whose `Tools/` folder could not be listed.
    pub unread: Vec<UnreadPresetStore>,
}

/// Whose walk, and for which tool (v4 `ToolPresetPoolContext`).
#[derive(Debug, Clone, Default)]
pub struct ToolPresetPoolContext {
    pub user_id: String,
    pub character_id: Option<String>,
    pub character_mount_point_id: Option<String>,
    pub project_id: Option<String>,
    pub tool_name: String,
}

fn tier_str(tier: MountTier) -> &'static str {
    match tier {
        MountTier::Character => "character",
        MountTier::Participant => "participant",
        MountTier::Group => "group",
        MountTier::Project => "project",
        MountTier::Global => "global",
    }
}

impl ToolPresetStore {
    /// v4's key order: `tier, mountPointId, mountName`.
    pub fn to_value(&self) -> Value {
        json!({
            "tier": tier_str(self.tier),
            "mountPointId": self.mount_point_id,
            "mountName": self.mount_name,
        })
    }
}

impl ToolPreset {
    /// v4's key order (`{ ...store, name, path, shadows }`): `tier,
    /// mountPointId, mountName, name, path, shadows`.
    pub fn to_value(&self) -> Value {
        let mut v = self.store.to_value();
        let obj = v.as_object_mut().expect("a store renders as an object");
        obj.insert("name".into(), Value::String(self.name.clone()));
        obj.insert("path".into(), Value::String(self.path.clone()));
        obj.insert(
            "shadows".into(),
            Value::Array(self.shadows.iter().map(ToolPresetStore::to_value).collect()),
        );
        v
    }
}

impl UnreadPresetStore {
    /// v4's key order (`{ ...store, reason }`): `tier, mountPointId, mountName,
    /// reason`.
    pub fn to_value(&self) -> Value {
        let mut v = self.store.to_value();
        v.as_object_mut()
            .expect("a store renders as an object")
            .insert("reason".into(), Value::String(self.reason.clone()));
        v
    }
}

impl ToolPresetPool {
    /// The route's 200 body (contract C11): `{ presets, stores, unread }`.
    pub fn to_value(&self) -> Value {
        json!({
            "presets": self.presets.iter().map(ToolPreset::to_value).collect::<Vec<_>>(),
            "stores": self.stores.iter().map(ToolPresetStore::to_value).collect::<Vec<_>>(),
            "unread": self.unread.iter().map(UnreadPresetStore::to_value).collect::<Vec<_>>(),
        })
    }
}

/// Walk an already-resolved pool for one tool's presets (v4
/// `resolveToolPresetPool` after its pool call).
///
/// - `find_mount(id)` is v4's `docMountPoints.findById`: `Ok(None)` (a vanished
///   row) skips the store SILENTLY — no line, not a save destination; an `Err`
///   propagates (see the module doc). `Ok(Some((name, row)))` hands the row on
///   to `list`.
/// - `list(&row)` is v4's `listToolsFolderFiles`; its `Err` is the message text.
///
/// Logs v4's WARN `Preset listing failed for store` per unlisted store and one
/// DEBUG `Tool presets resolved` at the end.
pub fn resolve_tool_preset_pool_from<M, E>(
    pool: &TieredMountPool,
    tool_name: &str,
    character_id: Option<&str>,
    mut find_mount: impl FnMut(&str) -> Result<Option<(String, M)>, E>,
    mut list: impl FnMut(&M) -> Result<Vec<String>, String>,
) -> Result<ToolPresetPool, E> {
    let mut presets: Vec<ToolPreset> = Vec::new();
    let mut stores: Vec<ToolPresetStore> = Vec::new();
    let mut unread: Vec<UnreadPresetStore> = Vec::new();

    for (tier, mount_point_id) in ordered_mounts(pool) {
        let Some((mount_name, row)) = find_mount(&mount_point_id)? else {
            continue;
        };

        let store = ToolPresetStore {
            tier,
            mount_point_id: mount_point_id.clone(),
            mount_name,
        };
        // Pushed BEFORE the listing: a store whose folder will not list is still
        // a place a preset may be filed — the listing failed, not the store.
        stores.push(store.clone());

        let paths = match list(&row) {
            Ok(paths) => paths,
            Err(error) => {
                tracing::warn!(
                    target: "quilltap::pascal",
                    context = CONTEXT,
                    mountPointId = %store.mount_point_id,
                    mountName = %store.mount_name,
                    error = %error,
                    "Preset listing failed for store",
                );
                unread.push(UnreadPresetStore {
                    store,
                    reason: error,
                });
                continue;
            }
        };

        for relative_path in paths {
            let Some(name) = parse_preset_from_path(tool_name, &relative_path) else {
                continue;
            };
            // A nearer store already won this name: this one is a shadow.
            if let Some(nearer) = presets.iter_mut().find(|p| p.name == name) {
                nearer.shadows.push(store.clone());
                continue;
            }
            presets.push(ToolPreset {
                store: store.clone(),
                name,
                path: relative_path,
                shadows: Vec::new(),
            });
        }
    }

    // v4 `[...byName.values()].sort((a, b) => a.name.localeCompare(b.name))` — ICU
    // collation, not code-unit order (`_` and `-` sort before digits), and a
    // stable sort over the Map's insertion order.
    presets.sort_by(|a, b| locale_compare(&a.name, &b.name));

    let shadowed: usize = presets.iter().map(|p| p.shadows.len()).sum();
    // v4 logs `characterId: ctx.characterId ?? null`; a `null` renders absent on
    // the capture rig, so `None` omits the field.
    match character_id {
        Some(character_id) => tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            toolName = tool_name,
            characterId = character_id,
            stores = stores.len() as u64,
            presets = presets.len() as u64,
            shadowed = shadowed as u64,
            unread = unread.len() as u64,
            "Tool presets resolved",
        ),
        None => tracing::debug!(
            target: "quilltap::pascal",
            context = CONTEXT,
            toolName = tool_name,
            stores = stores.len() as u64,
            presets = presets.len() as u64,
            shadowed = shadowed as u64,
            unread = unread.len() as u64,
            "Tool presets resolved",
        ),
    }

    Ok(ToolPresetPool {
        presets,
        stores,
        unread,
    })
}

/// Resolve one tool's presets for one invoker against the live DB (v4
/// `resolveToolPresetPool`).
///
/// Same pool as the roster's — no participant tier, since another character's
/// vault is theirs alone — but with `enabled: require` ([`PRESET_POOL_OPTIONS`]).
/// A mount-row read error propagates (v4's `findById` sits outside its `try`).
pub fn resolve_tool_preset_pool(
    ctx: &ToolPresetPoolContext,
    main: &Connection,
    mount: &Connection,
) -> Result<ToolPresetPool, DbError> {
    let (roster, enabled) = PRESET_POOL_OPTIONS;
    let pool = resolve_tiered_mount_pool(
        main,
        mount,
        &TierContext {
            user_id: Some(ctx.user_id.clone()),
            character_id: ctx.character_id.clone(),
            character_mount_point_id: ctx.character_mount_point_id.clone(),
            character_ids: None,
            project_id: ctx.project_id.clone(),
        },
        &TierResolveOptions::new(roster, enabled),
    );

    let repo = DocMountPointsRepository::new(mount);
    resolve_tool_preset_pool_from(
        &pool,
        &ctx.tool_name,
        ctx.character_id.as_deref(),
        |id| {
            Ok::<_, DbError>(
                repo.find_service_info_by_id(id)?
                    .map(|row: MountServiceInfo| (row.name.clone(), row)),
            )
        },
        |row| list_tools_folder_files(mount, row),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wrapper resolves the pool with v4's `{ roster: 'open', enabled:
    /// 'require' }` (the oracle's `poolCallOptions` is compared against the same
    /// strings by the differential).
    #[test]
    fn the_preset_pool_states_open_roster_and_required_enabled() {
        assert_eq!(PRESET_POOL_OPTIONS, (Roster::Open, Enabled::Require));
    }
}
