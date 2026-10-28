//! v4's PHASE 3.4 / 3.4b scenario heals — `ensureProjectScenariosForAllProjects`
//! (`lib/startup/ensure-project-scenarios.ts`) and
//! `ensureGroupScenariosForAllGroups` (`lib/startup/ensure-group-scenarios.ts`)
//! — ported by P4.171 (C-19, the P4.D295 R-Y deferral). The host runs them at
//! boot in v4's phase order, ahead of the General heal (3.4c).
//!
//! Each loop walks the owners' RAW rows (the store overlay would throw or drop a
//! storeless owner; the hook needs only `id` / `name` /
//! `officialMountPointId`) and resolves each owner's scenario location with
//! `ensure`, which finds, adopts or creates the official store
//! (`ensure_official_store`, step 2 included) and heals `Scenarios/` inside it.
//! A folder failure throws and is COUNTED as a failure rather than reported as
//! healthy. The group loop also heals the group's `Knowledge/` folder.
//!
//! v4's `failed to list` ERROR arm (a `findAllRaw` that throws) has no twin:
//! `findAllRaw` is a fallback read on both sides (`_findAll` answers `[]` with
//! its own line), so neither side can reach it.
//!
//! ⚠ NOT ported here (recorded by P4.171, Tier 3): v4's two store backfills —
//! PHASE 3.4a `backfillProjectStores` and the first half of 3.4b,
//! `backfillGroupStores` (`lib/startup/backfill-{project,group}-stores.ts`).
//! Their populate step writes the four store files from v4's WIDE `_findAll`
//! row (the legacy store-resident columns coerced through `ProjectSchema` /
//! `GroupSchema`), a read v5's raw finders do not carry.

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::{groups, projects};
use crate::scenarios::location::{resolve_scenario_location, ScenarioScope};

/// v4's `{ total, provisioned, healed, failed }` result of either loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScenarioHealCounts {
    pub total: usize,
    /// A store was found, adopted or created for an owner that recorded none.
    pub provisioned: usize,
    /// The recorded store pointer was stale and has been replaced.
    pub healed: usize,
    /// No store could be resolved, or a folder could not be created.
    pub failed: usize,
}

/// v4 `ensureProjectScenariosForAllProjects()` (PHASE 3.4).
pub fn ensure_project_scenarios_for_all_projects(
    main: &Connection,
    mount: &Connection,
) -> ScenarioHealCounts {
    let mut counts = ScenarioHealCounts::default();
    let owners = projects::find_all_raw(main);
    counts.total = owners.len();
    for project in &owners {
        let (id, name, fk) = owner_fields(project);
        match heal_one(main, mount, ScenarioScope::Project, id, fk, &mut counts) {
            Ok(_) => {}
            Err(error) => {
                counts.failed += 1;
                tracing::warn!(
                    projectId = %id,
                    projectName = %name,
                    errorJson = %json_string(&error),
                    "ensure-project-scenarios: failed for project; continuing"
                );
            }
        }
    }
    tracing::info!(
        total = counts.total,
        provisioned = counts.provisioned,
        healed = counts.healed,
        failed = counts.failed,
        "ensure-project-scenarios: complete"
    );
    counts
}

/// v4 `ensureGroupScenariosForAllGroups()` (PHASE 3.4b, after the group store
/// backfill): the project loop's twin, plus `ensureGroupKnowledgeFolder` on
/// each resolved store.
pub fn ensure_group_scenarios_for_all_groups(
    main: &Connection,
    mount: &Connection,
) -> ScenarioHealCounts {
    let mut counts = ScenarioHealCounts::default();
    let owners = groups::find_all_raw(main);
    counts.total = owners.len();
    for group in &owners {
        let (id, name, fk) = owner_fields(group);
        let healed = heal_one(main, mount, ScenarioScope::Group, id, fk, &mut counts).and_then(
            |mp| match mp {
                Some(mp) => crate::api::scenarios::ensure_group_knowledge_folder(mount, &mp)
                    .map_err(|e| e.to_string()),
                None => Ok(()),
            },
        );
        if let Err(error) = healed {
            counts.failed += 1;
            tracing::warn!(
                groupId = %id,
                groupName = %name,
                errorJson = %json_string(&error),
                "ensure-group-scenarios: failed for group; continuing"
            );
        }
    }
    tracing::info!(
        total = counts.total,
        provisioned = counts.provisioned,
        healed = counts.healed,
        failed = counts.failed,
        "ensure-group-scenarios: complete"
    );
    counts
}

/// v4 passes `error` as a context STRING (`error.message`); a tracing field
/// named `error` would become the file envelope's error object, so the line
/// carries it under the `…Json` convention (§R.5, `log_file.rs`).
fn json_string(s: &str) -> String {
    serde_json::to_string(s).expect("a string serializes")
}

/// The raw row's `id`, `name` and `officialMountPointId` (`None` when falsy —
/// v4's `if (!owner.officialMountPointId)`).
fn owner_fields(row: &Map<String, Value>) -> (&str, &str, Option<&str>) {
    let s = |k: &str| row.get(k).and_then(Value::as_str);
    (
        s("id").unwrap_or(""),
        s("name").unwrap_or(""),
        s("officialMountPointId").filter(|v| !v.is_empty()),
    )
}

/// One owner's `resolveScenarioLocation(scope, id, …, { ensure: true })` and
/// v4's tally: no location → `failed`; no recorded store → `provisioned`; a
/// recorded store other than the resolved one → `healed`. `Ok(Some(mp))`
/// carries the resolved store for the group loop's `Knowledge/` heal; an
/// `Err` is the thrown message (the caller counts and logs it).
fn heal_one(
    main: &Connection,
    mount: &Connection,
    scope: ScenarioScope,
    id: &str,
    recorded: Option<&str>,
    counts: &mut ScenarioHealCounts,
) -> Result<Option<String>, String> {
    let location = resolve_scenario_location(
        main,
        mount,
        scope,
        Some(id),
        crate::api::SINGLE_USER_ID,
        true,
    )
    .map_err(|e| e.to_string())?;
    let Some(location) = location else {
        counts.failed += 1;
        return Ok(None);
    };
    match recorded {
        None => counts.provisioned += 1,
        Some(fk) if fk != location.mount_point_id() => counts.healed += 1,
        Some(_) => {}
    }
    Ok(Some(location.mount_point_id().to_string()))
}
