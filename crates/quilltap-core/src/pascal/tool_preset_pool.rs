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
//! # A mount row that will not read is a skipped store
//!
//! v4's docblock says the walk never throws, and a store that will not LIST is
//! reported in `unread`. The mount-row read (`docMountPoints.findById`) sits
//! outside the `try`, but it cannot throw either: `DocMountPointsRepository`
//! does not override base `_findById`, a FALLBACK `safeQuery`
//! (`base.repository.ts:247-258`) — a failed read logs the repository's ERROR
//! `Error finding entity by ID {collection: doc_mount_points, id}` and answers
//! `null`, so the store is skipped like a vanished row and the route answers
//! 200. v5's live wiring reads through [`crate::db::fallback::find_by_id_or_none`]
//! (the read [`crate::db::tiered_mount_pool`]'s `drop_disabled_mounts` makes for
//! the same rows).

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
///   row, or a failed read after its repository line — see the module doc)
///   skips the store, not a save destination; an `Err` propagates (the live
///   wiring never returns one). `Ok(Some((name, row)))` hands the row on to
///   `list`.
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
/// A mount-row read error skips that store after the repository's line (v4's
/// `findById` is a fallback read — see the module doc).
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

    resolve_tool_preset_pool_from(
        &pool,
        &ctx.tool_name,
        ctx.character_id.as_deref(),
        |id| find_preset_mount(mount, id),
        |row| list_tools_folder_files(mount, row),
    )
}

/// v4's `repos.docMountPoints.findById(mountPointId)` for one pool store: base
/// `_findById`, a fallback read — a failed read logs `Error finding entity by
/// ID` and answers `None` (the store is skipped), never an `Err`.
fn find_preset_mount(
    mount: &Connection,
    id: &str,
) -> Result<Option<(String, MountServiceInfo)>, DbError> {
    let repo = DocMountPointsRepository::new(mount);
    Ok(
        crate::db::fallback::find_by_id_or_none("doc_mount_points", id, || {
            repo.find_service_info_by_id(id)
        })
        .map(|row: MountServiceInfo| (row.name.clone(), row)),
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

    /// v4's `docMountPoints.findById` is base `_findById` — a FALLBACK
    /// `safeQuery` (`base.repository.ts:247-258`; `DocMountPointsRepository`
    /// does not override it): a failed mount-row read logs the repository's
    /// `Error finding entity by ID {collection: doc_mount_points, id}`, answers
    /// `null`, and the walk skips that store — the route still answers 200. A
    /// NULL `name` makes v5's row read fail; the readable store survives.
    #[test]
    fn a_failing_mount_read_skips_the_store_and_logs_the_repository_line() {
        let mount = Connection::open_in_memory().unwrap();
        mount
            .execute_batch(
                r#"CREATE TABLE "doc_mount_points" ("id" TEXT PRIMARY KEY NOT NULL,
                     "mountType" TEXT, "basePath" TEXT, "excludePatterns" TEXT,
                     "includePatterns" TEXT, "name" TEXT, "enabled" INTEGER,
                     "conversionStatus" TEXT);
                   INSERT INTO doc_mount_points VALUES
                     ('m-bad', 'database', '', '[]', '[]', NULL, 1, 'idle'),
                     ('m-good', 'database', '', '[]', '[]', 'Good Store', 1, 'idle');"#,
            )
            .unwrap();
        let pool = TieredMountPool {
            group_mount_point_ids: vec!["m-bad".into()],
            global_mount_point_id: Some("m-good".into()),
            ..Default::default()
        };
        let (result, lines) = crate::test_support::captured_with(|| {
            resolve_tool_preset_pool_from(
                &pool,
                "roll",
                None,
                |id| find_preset_mount(&mount, id),
                |_| Ok(vec!["Tools/roll.fast.settings.json".to_string()]),
            )
        });
        let pool = result.expect("a failed mount read is not the walk's Err");
        let ids: Vec<&str> = pool
            .stores
            .iter()
            .map(|s| s.mount_point_id.as_str())
            .collect();
        assert_eq!(ids, ["m-good"], "{lines:?}");
        assert_eq!(pool.presets.len(), 1);
        let home: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("Error finding entity by ID"))
            .collect();
        assert_eq!(home.len(), 1, "{lines:?}");
        assert!(
            home[0].starts_with(
                "ERROR quilltap::db Error finding entity by ID collection=doc_mount_points id=m-bad"
            ),
            "{}",
            home[0]
        );
    }
}
