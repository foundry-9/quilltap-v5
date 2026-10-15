//! Scenarios everywhere — v4 `lib/scenarios/everywhere.ts` (`545e299bb`,
//! pinned at `83334b9a5`): every scenario in the instance, grouped by owner, as
//! excerpts.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::collation::locale_compare_base;
use crate::db::doc_mount_documents::{DocMountDocumentsRepository, VaultFolderDoc};
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::scenarios::{build_scenario_list, ScenarioListLogContext, SCENARIOS_FOLDER};
use crate::db::{characters_read, groups, projects, DbError};
use crate::jsstr::{is_js_ws, js_trim, js_trim_end, utf16_truncate};
use crate::scenarios::location::{
    character_scenario_id_for_path, parse_scenario_for_scope, ScenarioScope, GENERAL_OWNER_NAME,
};

/// v4 `SCENARIO_EXCERPT_LENGTH` (`everywhere.ts:36`): characters of a body an
/// "everywhere" row carries in lieu of the body itself — UTF-16 code units, as
/// JavaScript counts them.
pub const SCENARIO_EXCERPT_LENGTH: usize = 240;

/// v4 `scenarioExcerpt(body)` (`everywhere.ts:70-75`): collapse every run of
/// JavaScript `\s` (the Unicode whitespace class — NBSP, U+FEFF and U+2028
/// included) to one space, trim, and cut past 240 code units to
/// `slice(0, 240).trimEnd() + '…'`.
///
/// ⚠ Ruled divergence `EXCERPT_LONE_SURROGATE` (R-AA): v4's `slice` counts
/// UTF-16 units and can cut a surrogate pair in half, leaving a lone high
/// surrogate at unit 239 that a Rust `String` cannot hold. v5 emits U+FFFD in
/// its place (one unit, the same position), so every excerpt is unit-for-unit
/// v4's except that one unit — pinned both ways by `scenario_pure_equivalence`
/// so a v4 convergence trips it.
pub fn scenario_excerpt(body: &str) -> String {
    let mut collapsed = String::with_capacity(body.len());
    let mut in_ws = false;
    for c in body.chars() {
        if is_js_ws(c) {
            if !in_ws {
                collapsed.push(' ');
                in_ws = true;
            }
        } else {
            collapsed.push(c);
            in_ws = false;
        }
    }
    let collapsed = js_trim(&collapsed);
    if collapsed.encode_utf16().count() > SCENARIO_EXCERPT_LENGTH {
        let cut = utf16_truncate(collapsed, SCENARIO_EXCERPT_LENGTH);
        format!("{}…", js_trim_end(&cut))
    } else {
        collapsed.to_string()
    }
}

/// One owner with a scenario shelf (v4 `Owner`, `everywhere.ts:61-67`).
struct Owner {
    scope: ScenarioScope,
    id: Option<String>,
    name: String,
    mount_point_id: String,
    default_scenario_id: Option<String>,
}

/// v4 `byName` (`everywhere.ts:77-78`): `localeCompare(…, { sensitivity:
/// 'base' })` — `sort_by` is stable, as V8's sort is, so a case-insensitive
/// tie keeps the raw finders' (rowid) input order.
fn by_name(a: &Owner, b: &Owner) -> std::cmp::Ordering {
    locale_compare_base(&a.name, &b.name)
}

/// v4 `collectOwners(repos, userId)` (`everywhere.ts:81-127`): General first
/// (when its pointer exists), then projects, then groups — rows with a truthy
/// `officialMountPointId`, named `name || 'Project' | 'Group'`, NOT
/// user-filtered — then the caller's live, vaulted characters, each kind sorted
/// by name. Raw reads throughout; nothing is provisioned.
fn collect_owners(main: &Connection, user_id: &str) -> Result<Vec<Owner>, DbError> {
    let mut owners: Vec<Owner> = Vec::new();
    if let Some(mount_point_id) = get_general_mount_point_id(main)? {
        owners.push(Owner {
            scope: ScenarioScope::General,
            id: None,
            name: GENERAL_OWNER_NAME.to_string(),
            mount_point_id,
            default_scenario_id: None,
        });
    }

    let store_owners = |scope: ScenarioScope, rows: Vec<Map<String, Value>>, fallback: &str| {
        let mut out: Vec<Owner> = rows
            .into_iter()
            .filter_map(|row| {
                let mount_point_id = row
                    .get("officialMountPointId")
                    .and_then(Value::as_str)
                    .filter(|m| !m.is_empty())?
                    .to_string();
                let name = row
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|n| !n.is_empty())
                    .unwrap_or(fallback)
                    .to_string();
                Some(Owner {
                    scope,
                    id: row.get("id").and_then(Value::as_str).map(str::to_string),
                    name,
                    mount_point_id,
                    default_scenario_id: None,
                })
            })
            .collect();
        out.sort_by(by_name);
        out
    };
    owners.extend(store_owners(
        ScenarioScope::Project,
        projects::find_all_raw(main),
        "Project",
    ));
    owners.extend(store_owners(
        ScenarioScope::Group,
        groups::find_all_raw(main),
        "Group",
    ));

    let mut characters: Vec<Owner> = characters_read::find_all_raw_or_empty(main)
        .into_iter()
        .filter_map(|c| {
            let s = |k: &str| c.get(k).and_then(Value::as_str);
            // Archived characters are tombstones; a vaultless one has no shelf.
            let archived = c.get("archivedAt").is_some_and(|v| match v {
                Value::Null => false,
                Value::String(s) => !s.is_empty(),
                Value::Bool(b) => *b,
                _ => true,
            });
            if s("userId") != Some(user_id) || archived {
                return None;
            }
            let mount_point_id = s("characterDocumentMountPointId").filter(|m| !m.is_empty())?;
            Some(Owner {
                scope: ScenarioScope::Character,
                id: s("id").map(str::to_string),
                name: s("name").unwrap_or_default().to_string(),
                mount_point_id: mount_point_id.to_string(),
                default_scenario_id: s("defaultScenarioId").map(str::to_string),
            })
        })
        .collect();
    characters.sort_by(by_name);
    owners.extend(characters);
    Ok(owners)
}

/// v4 `listScenariosEverywhere(repos, userId, { includeArchived })`
/// (`everywhere.ts:133-188`): every scenario in the instance, grouped by owner
/// in browser order, archived omitted unless asked for. ONE folder query over
/// the deduped mounts, bucketed by mount; DEBUG `[ScenariosEverywhere] Listed
/// scenario shelves {owners, mounts, documents, includeArchived}`; per owner
/// `buildScenarioList` with the scope's parser — an EMPTY shelf is included.
/// Each source is `{scope, id, name, mountPointId, scenarios, warnings}`; each
/// row `{path, scenarioId? (character only), filename, name, description?,
/// isDefault, archived, excerpt, updatedAt}` — the excerpt, never the body.
pub fn list_scenarios_everywhere(
    main: &Connection,
    mount: &Connection,
    user_id: &str,
    include_archived: bool,
) -> Result<Vec<Value>, DbError> {
    let owners = collect_owners(main, user_id)?;
    let mut mount_ids: Vec<String> = Vec::new();
    for o in &owners {
        if !mount_ids.contains(&o.mount_point_id) {
            mount_ids.push(o.mount_point_id.clone());
        }
    }
    let docs = DocMountDocumentsRepository::new(mount)
        .find_many_by_mount_points_in_folder_or_empty(&mount_ids, SCENARIOS_FOLDER, ".md")?;
    let document_count = docs.len();
    let mut by_mount: HashMap<String, Vec<VaultFolderDoc>> = HashMap::new();
    for doc in docs {
        by_mount
            .entry(doc.mount_point_id.clone())
            .or_default()
            .push(doc);
    }

    tracing::debug!(
        owners = owners.len(),
        mounts = mount_ids.len(),
        documents = document_count,
        includeArchived = include_archived,
        "[ScenariosEverywhere] Listed scenario shelves"
    );

    Ok(owners
        .into_iter()
        .map(|owner| {
            let docs = by_mount
                .get(&owner.mount_point_id)
                .cloned()
                .unwrap_or_default();
            let listed = build_scenario_list(
                docs,
                |d| parse_scenario_for_scope(owner.scope, d, owner.default_scenario_id.as_deref()),
                include_archived,
                &ScenarioListLogContext {
                    scope: owner.scope.as_str().to_string(),
                    id: owner.id.clone(),
                    mount_point_id: owner.mount_point_id.clone(),
                },
            );
            let scenarios: Vec<Value> = listed
                .scenarios
                .iter()
                .map(|s| {
                    let mut row = Map::new();
                    row.insert("path".into(), json!(s.path));
                    if owner.scope == ScenarioScope::Character {
                        row.insert(
                            "scenarioId".into(),
                            json!(character_scenario_id_for_path(
                                &owner.mount_point_id,
                                &s.path
                            )),
                        );
                    }
                    row.insert("filename".into(), json!(s.filename));
                    row.insert("name".into(), json!(s.name));
                    if let Some(d) = &s.description {
                        row.insert("description".into(), json!(d));
                    }
                    row.insert("isDefault".into(), json!(s.is_default));
                    row.insert("archived".into(), json!(s.archived));
                    row.insert("excerpt".into(), json!(scenario_excerpt(&s.body)));
                    row.insert("updatedAt".into(), json!(s.updated_at));
                    Value::Object(row)
                })
                .collect();
            let mut source = Map::new();
            source.insert("scope".into(), json!(owner.scope.as_str()));
            source.insert("id".into(), owner.id.map_or(Value::Null, Value::String));
            source.insert("name".into(), json!(owner.name));
            source.insert("mountPointId".into(), json!(owner.mount_point_id));
            source.insert("scenarios".into(), Value::Array(scenarios));
            source.insert("warnings".into(), json!(listed.warnings));
            Value::Object(source)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L19 (`[General v1] Failed to list scenarios everywhere`) is v4's catch
    /// around `listScenariosEverywhere` — UNREACHABLE through v4's real code:
    /// every read it makes is a fallback (`getGeneralMountPointId`'s setting
    /// read, the three `findAllRaw`s, `findManyByMountPointsInFolder`), so a
    /// broken database lists nothing rather than throwing. Pinned NEGATIVE: the
    /// port's reads are the same fallbacks, so a table-less instance answers
    /// `Ok([])` and the route's L19 arm stays dead in both stacks.
    #[test]
    fn a_broken_instance_lists_nothing_rather_than_throwing() {
        let main = Connection::open_in_memory().unwrap();
        let mount = Connection::open_in_memory().unwrap();
        let got = list_scenarios_everywhere(&main, &mount, "u", false);
        assert_eq!(got.expect("every read is a fallback"), Vec::<Value>::new());
    }
}
