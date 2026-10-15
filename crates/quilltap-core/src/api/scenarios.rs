//! The scenario route factory + the general (instance-wide) scenarios family —
//! v4 `lib/mount-index/scenario-item-route-factory.ts` (`545e299bb`, pinned at
//! `83334b9a5`) and `app/api/v1/scenarios/**`.
//!
//! v4's General, project and group scenario endpoints are ONE factory
//! configured three ways (`ScenarioItemRouteConfig {scope, ownerLabel,
//! logTag, logIdKey, afterEnsure}`); every read and write goes through the
//! scope's location ([`crate::scenarios::location`]), so the routes hold no
//! file IO of their own. [`ScenarioRouteConfig`] is that config; [`GENERAL`],
//! `api::projects::PROJECT_SCENARIO_ROUTES` and
//! `api::groups::GROUP_SCENARIO_ROUTES` are the three.
//!
//! The factory's rules, all v4's:
//! - **A read never provisions** (the item GET's location is a probe — an
//!   owner without a store is a 404 naming the remedy), and a write never
//!   mints a store: the item PUT / rename / DELETE probe first, then (only when
//!   the probe found a store) re-resolve with `ensure` to HEAL `Scenarios/`
//!   (+ the group's `Knowledge/`). The collection list / create DO ensure.
//! - **Writes re-read through `read`**: a write whose file then parses to
//!   nothing (a whitespace-only body passes `body.min(1)`) answers 404 AFTER
//!   the write (Δ12); a same-name rename of a missing file is a 404 (Δ11).
//! - **The collection create parses the body FIRST** (Δ13): an invalid body
//!   for a missing owner is a 400.
//! - **The owner checks read the RAW row** (Δ10); the store is named
//!   `name || 'Project' | 'Group'`.
//! - **The lines** (L16–L18, never ported before P4.D295): INFO `<tag>
//!   Updated|Renamed|Deleted|Created <owner> scenario {<logIdKey>, userId,
//!   mountPointId, …}` and ERROR `<tag> Failed to <verb> <owner> scenario(s)
//!   {<logIdKey>[, scenarioPath]}` + the error, General's `ownerId: null`
//!   first.
//!
//! The item routes keep the boundary seam (no decode) on the route's path
//! segment; the location decodes once (L-6 — see
//! [`crate::scenarios::location::normalize_scenario_path`]).
//!
//! Every handler runs inside the writer closure (`with_both_conns`), so its
//! lines land on the writer thread — the harness pins them through a global
//! capture.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::db::chats::ScenarioRef;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::runtime::Db;
use crate::db::scenarios::{
    resolve_scenario_path, ListScenariosResult, ScenarioPathResolution, SCENARIOS_FOLDER,
};
use crate::db::{groups, projects};
use crate::jsstr::{zod_len_max_ok, zod_len_min_ok};
use crate::scenarios::everywhere::list_scenarios_everywhere;
use crate::scenarios::location::{
    lookup_scenario_ref, resolve_store_scenario_location, CreateScenarioInput,
    ScenarioLocationError, ScenarioRefLookup, ScenarioScope, StoreScenarioLocation,
    UpdateScenarioInput,
};
use crate::services::image_job_common::with_both_conns;

use super::types::{ErrorKind, Response};

// ===========================================================================
// Response helpers
// ===========================================================================

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}

// ===========================================================================
// Scenario-bag validation (the Zod schemas duplicated across the route files)
// ===========================================================================

/// The fields shared by create/update after validation.
struct ScenarioFields {
    name: Option<String>,
    description: Option<String>,
    is_default: Option<bool>,
    /// v4 `d25dacc1`'s `archived: z.boolean().optional()` — a TRI-STATE on the
    /// update path: omitted PRESERVES the file's current flag (the serializer
    /// rewrites the whole file, so an unmentioned flag would be dropped), `true`
    /// archives, `false` restores.
    archived: Option<bool>,
    body: String,
}

/// Collect the Zod-issue messages a create/update bag would raise, in the
/// schema's key order (name, description, isDefault, body; filename first for
/// create). Only the tested paths need byte-exact messages — chiefly the
/// empty-`body` custom message `Scenario body cannot be empty`.
fn collect_bag_issues(bag: &Value, include_filename: bool) -> Vec<String> {
    let mut issues: Vec<String> = Vec::new();
    let Some(obj) = bag.as_object() else {
        issues.push("Expected object, received other".to_string());
        return issues;
    };

    if include_filename {
        match obj.get("filename") {
            // `z.string().min(1).max(100)` under Zod ≥ 4.5.4's code-point windows (v4 `6e1a64ea6`).
            Some(Value::String(s)) if zod_len_min_ok(s, 1) && zod_len_max_ok(s, 100) => {}
            Some(Value::String(_)) => issues.push("Invalid filename length".to_string()),
            _ => issues.push("Required".to_string()),
        }
    }
    // name?: string 1..=200 when present.
    if let Some(v) = obj.get("name") {
        if !v.is_null() {
            match v {
                Value::String(s) if zod_len_min_ok(s, 1) && zod_len_max_ok(s, 200) => {}
                _ => issues.push("Invalid name".to_string()),
            }
        }
    }
    // description?: string ≤500 when present.
    if let Some(v) = obj.get("description") {
        if !v.is_null() {
            match v {
                Value::String(s) if zod_len_max_ok(s, 500) => {}
                _ => issues.push("Invalid description".to_string()),
            }
        }
    }
    // isDefault?: boolean when present.
    if let Some(v) = obj.get("isDefault") {
        if !v.is_null() && !v.is_boolean() {
            issues.push("Invalid isDefault".to_string());
        }
    }
    // archived?: boolean when present (v4 `d25dacc1`). Zod's `.optional()`
    // accepts an ABSENT key, never an explicit null — a present non-boolean
    // (null included) refuses with Zod 4's own sentence, measured on v4's
    // zod 4.4.3 (unify §3). ⚠ The sibling name/description/isDefault arms
    // keep their older null-tolerant spelling — a recorded pre-existing LEAD
    // of the same class, not this round's field.
    if let Some(v) = obj.get("archived") {
        if !v.is_boolean() {
            issues.push(format!(
                "Invalid input: expected boolean, received {}",
                zod_type_name(v)
            ));
        }
    }
    // body: string min 1 — the custom message is byte-faithful (tested).
    match obj.get("body") {
        Some(Value::String(s)) if !s.is_empty() => {}
        Some(Value::String(_)) => issues.push("Scenario body cannot be empty".to_string()),
        _ => issues.push("Required".to_string()),
    }
    issues
}

/// Zod 4's received-type word for its `Invalid input: expected …, received …`
/// sentences (measured on v4's zod 4.4.3).
fn zod_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Extract the validated fields (call only after `collect_bag_issues` is empty).
fn extract_fields(bag: &Value) -> ScenarioFields {
    ScenarioFields {
        name: bag.get("name").and_then(Value::as_str).map(str::to_string),
        description: bag
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_default: bag.get("isDefault").and_then(Value::as_bool),
        archived: bag.get("archived").and_then(Value::as_bool),
        body: bag
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
}

// ===========================================================================
// The factory (v4 `scenario-item-route-factory.ts`)
// ===========================================================================

/// v4's `logIdKey` — the metadata key naming the owner id on the factory's
/// lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerLogKey {
    OwnerId,
    ProjectId,
    GroupId,
}

/// v4 `ScenarioItemRouteConfig` (`scenario-item-route-factory.ts:45-57`).
#[derive(Clone, Copy)]
pub struct ScenarioRouteConfig {
    /// The store scope this endpoint serves.
    pub scope: ScenarioScope,
    /// Capitalised owner label — `'General'` / `'Project'` / `'Group'`.
    pub owner_label: &'static str,
    /// Log-message prefix — `'[General v1]'` / `'[Projects v1]'` / `'[Groups v1]'`.
    pub log_tag: &'static str,
    pub log_id_key: OwnerLogKey,
    /// v4's `afterEnsure` — the group tier heals `Knowledge/` beside
    /// `Scenarios/` once the store is known for a write.
    pub ensure_knowledge: bool,
    /// The family's response envelope.
    pub wrap: fn(Value) -> Response,
}

impl ScenarioRouteConfig {
    /// v4 `ownerLabel.toLowerCase()`.
    fn owner(&self) -> String {
        self.owner_label.to_lowercase()
    }
}

/// The General family's config (`scenarios/route.ts:40-45`).
pub const GENERAL: ScenarioRouteConfig = ScenarioRouteConfig {
    scope: ScenarioScope::General,
    owner_label: "General",
    log_tag: "[General v1]",
    log_id_key: OwnerLogKey::OwnerId,
    ensure_knowledge: false,
    wrap: Response::Scenario,
};

/// One factory line with its owner-id key first (`{ [logIdKey]: id ?? null,
/// … }`) — a tracing field NAME must be static, so the key is matched.
macro_rules! owner_line {
    ($level:ident, $cfg:expr, $owner_id:expr; $($rest:tt)*) => {{
        let owner_id: &str = $owner_id.unwrap_or("null");
        match $cfg.log_id_key {
            OwnerLogKey::OwnerId => tracing::$level!(ownerId = owner_id, $($rest)*),
            OwnerLogKey::ProjectId => tracing::$level!(projectId = owner_id, $($rest)*),
            OwnerLogKey::GroupId => tracing::$level!(groupId = owner_id, $($rest)*),
        }
    }};
}

/// v4 `scenarioErrorResponse(error)` (`:65-74`): a location error → its
/// response, or `None` when it isn't one of ours. (The ZodError arm is the
/// handlers' own `Invalid request body: …` 400, raised before the location.)
fn scenario_error_response(e: &ScenarioLocationError) -> Option<Response> {
    match e {
        ScenarioLocationError::NotFound(_) => Some(not_found("Scenario")),
        ScenarioLocationError::Conflict(_) | ScenarioLocationError::Filename(_) => {
            Some(bad_request(e.to_string()))
        }
        ScenarioLocationError::Uri(_) | ScenarioLocationError::Other(_) => None,
    }
}

/// v4 `badRequest('Invalid request body: <issues joined "; ">')`.
fn invalid_body(issues: Vec<String>) -> Response {
    bad_request(format!("Invalid request body: {}", issues.join("; ")))
}

/// v4 `renameScenarioSchema` (`scenarios-common.ts:79-81`): `newFilename:
/// z.string().min(1).max(100)` (code points), Zod 4's sentences.
fn rename_issues(new_filename: &str) -> Vec<String> {
    use crate::api::zod_issues::ZodIssue;
    if !zod_len_min_ok(new_filename, 1) {
        return vec![
            ZodIssue::too_small_string(json!(1), vec![json!("newFilename")])
                .message()
                .to_string(),
        ];
    }
    if !zod_len_max_ok(new_filename, 100) {
        return vec![
            ZodIssue::too_big_string(json!(100), vec![json!("newFilename")])
                .message()
                .to_string(),
        ];
    }
    Vec::new()
}

fn scenarios_value(listed: &ListScenariosResult) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(
        "scenarios".into(),
        serde_json::to_value(&listed.scenarios).unwrap_or_else(|_| json!([])),
    );
    m.insert("warnings".into(), json!(listed.warnings));
    m
}

/// Resolve a scenario path, or the 400 rejection Response (the route's
/// boundary seam — no decode, L-6).
pub(crate) fn resolve_path_or_400(scenario_path: &str) -> Result<String, Response> {
    match resolve_scenario_path(scenario_path, SCENARIOS_FOLDER) {
        ScenarioPathResolution::Ok(p) => Ok(p),
        ScenarioPathResolution::Err(e) => Err(bad_request(e)),
    }
}

/// v4 `GROUP_KNOWLEDGE_FOLDER` (`lib/mount-index/group-knowledge.ts`).
pub const GROUP_KNOWLEDGE_FOLDER: &str = "Knowledge";

/// v4 `ensureGroupKnowledgeFolder(mp)` (`group-knowledge.ts:17-22`): heal the
/// group's `Knowledge/` folder; no catch — a failure propagates to the caller.
pub(crate) fn ensure_group_knowledge_folder(
    mount: &Connection,
    mount_point_id: &str,
) -> Result<(), ScenarioLocationError> {
    DocMountFileLinksRepository::new(mount)
        .ensure_folder_path(mount_point_id, GROUP_KNOWLEDGE_FOLDER)
        .map(|_| ())
        .map_err(ScenarioLocationError::from)
}

/// v4 the owner's RAW row — the existence check of the item 404s and the
/// collection's owner gate.
fn owner_row_exists(main: &Connection, scope: ScenarioScope, owner_id: &str) -> bool {
    match scope {
        ScenarioScope::Project => projects::find_by_id_raw_or_none(main, owner_id).is_some(),
        _ => groups::find_by_id_raw_or_none(main, owner_id).is_some(),
    }
}

type Step<T> = Result<Result<T, Response>, ScenarioLocationError>;

/// v4 the item handlers' `loadLocation(ownerId, ctx, forWrite)` (`:91-116`):
/// PROBE; a write that found a store re-resolves with `ensure` (healing
/// `Scenarios/`, + `Knowledge/` for a group). Not found: General → 404
/// `Quilltap General mount has not been provisioned yet not found`; a missing
/// owner → `<Owner> not found`; a storeless owner → the remedy 404. A read
/// never heals; a write never mints a store.
fn load_location(
    main: &Connection,
    mount: &Connection,
    cfg: &ScenarioRouteConfig,
    owner_id: Option<&str>,
    user_id: &str,
    for_write: bool,
) -> Step<StoreScenarioLocation> {
    let mut location =
        resolve_store_scenario_location(main, mount, cfg.scope, owner_id, user_id, false)?;
    if location.is_some() && for_write {
        location =
            resolve_store_scenario_location(main, mount, cfg.scope, owner_id, user_id, true)?;
        if let (Some(l), true) = (&location, cfg.ensure_knowledge) {
            ensure_group_knowledge_folder(mount, l.mount_point_id())?;
        }
    }
    if let Some(l) = location {
        return Ok(Ok(l));
    }
    if cfg.scope == ScenarioScope::General {
        return Ok(Err(not_found(
            "Quilltap General mount has not been provisioned yet",
        )));
    }
    let exists = owner_id
        .filter(|i| !i.is_empty())
        .is_some_and(|id| owner_row_exists(main, cfg.scope, id));
    if !exists {
        return Ok(Err(not_found(cfg.owner_label)));
    }
    Ok(Err(not_found(&format!(
        "{} has no official document store yet — restart the server or call GET /scenarios first",
        cfg.owner_label
    ))))
}

/// v4 the collection bodies' `ensureLocation(ownerId, ctx, forWrite)`
/// (`:309-334`): the RAW owner check (404), then `ensure` — General
/// unprovisioned is a 400 for a write and an EMPTY 200 for a read; a store
/// that cannot be provisioned is a 500. Then `afterEnsure`.
fn ensure_location(
    main: &Connection,
    mount: &Connection,
    cfg: &ScenarioRouteConfig,
    owner_id: Option<&str>,
    user_id: &str,
    for_write: bool,
) -> Step<StoreScenarioLocation> {
    if cfg.scope != ScenarioScope::General {
        if let Some(id) = owner_id.filter(|i| !i.is_empty()) {
            if !owner_row_exists(main, cfg.scope, id) {
                return Ok(Err(not_found(cfg.owner_label)));
            }
        }
    }
    let Some(location) =
        resolve_store_scenario_location(main, mount, cfg.scope, owner_id, user_id, true)?
    else {
        if cfg.scope == ScenarioScope::General {
            return Ok(Err(if for_write {
                bad_request(
                    "Quilltap General mount has not been provisioned yet — restart the server",
                )
            } else {
                (cfg.wrap)(json!({ "mountPointId": null, "scenarios": [], "warnings": [] }))
            }));
        }
        return Ok(Err(internal(format!(
            "Failed to ensure {} document store",
            cfg.owner()
        ))));
    };
    if cfg.ensure_knowledge {
        ensure_group_knowledge_folder(mount, location.mount_point_id())?;
    }
    Ok(Ok(location))
}

/// Run one handler body on the writer and wrap its success in the family's
/// envelope.
async fn run<F>(db: &Db, cfg: ScenarioRouteConfig, f: F) -> Response
where
    F: FnOnce(&Connection, &Connection) -> Result<Value, Response> + Send + 'static,
{
    match with_both_conns(db, move |main, mount| Ok(f(main, mount))).await {
        Ok(Ok(body)) => (cfg.wrap)(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 the factory's item GET (`:122-146`).
pub async fn item_get(
    db: &Db,
    cfg: ScenarioRouteConfig,
    owner_id: Option<String>,
    scenario_path: String,
    user_id: String,
) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    run(db, cfg, move |main, mount| {
        let attempt = (|| -> Step<Value> {
            let location =
                match load_location(main, mount, &cfg, owner_id.as_deref(), &user_id, false)? {
                    Ok(l) => l,
                    Err(r) => return Ok(Err(r)),
                };
            Ok(match location.read(mount, &resolved)? {
                Some(scenario) => Ok(json!({ "scenario": scenario })),
                None => Err(not_found("Scenario")),
            })
        })();
        attempt.unwrap_or_else(|e| {
            owner_line!(error, cfg, owner_id.as_deref(); scenarioPath = %scenario_path, error = %e,
                "{} Failed to read {} scenario", cfg.log_tag, cfg.owner());
            Err(internal(format!("Failed to read {} scenario", cfg.owner())))
        })
    })
    .await
}

/// v4 the factory's item PUT (`:152-192`): heal on write, then the body
/// parse, update (404 when missing), re-list, INFO `Updated`.
pub async fn item_update(
    db: &Db,
    cfg: ScenarioRouteConfig,
    owner_id: Option<String>,
    scenario_path: String,
    bag: Value,
    include_archived: bool,
    user_id: String,
) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    run(db, cfg, move |main, mount| {
        let attempt = (|| -> Step<Value> {
            let location = match load_location(main, mount, &cfg, owner_id.as_deref(), &user_id, true)? {
                Ok(l) => l,
                Err(r) => return Ok(Err(r)),
            };
            let issues = collect_bag_issues(&bag, false);
            if !issues.is_empty() {
                return Ok(Err(invalid_body(issues)));
            }
            let fields = extract_fields(&bag);
            location.update(
                mount,
                &resolved,
                &UpdateScenarioInput {
                    name: fields.name,
                    description: fields.description,
                    is_default: fields.is_default,
                    archived: fields.archived,
                    body: fields.body,
                },
            )?;
            let listed = location.list(mount, include_archived)?;
            owner_line!(info, cfg, owner_id.as_deref(); userId = %user_id, mountPointId = %location.mount_point_id(),
                relativePath = %resolved, isDefault = fields.is_default == Some(true),
                "{} Updated {} scenario", cfg.log_tag, cfg.owner());
            Ok(Ok(Value::Object(scenarios_value(&listed))))
        })();
        attempt.unwrap_or_else(|e| {
            if let Some(r) = scenario_error_response(&e) {
                return Err(r);
            }
            owner_line!(error, cfg, owner_id.as_deref(); scenarioPath = %scenario_path, error = %e,
                "{} Failed to update {} scenario", cfg.log_tag, cfg.owner());
            Err(internal(format!("Failed to update {} scenario", cfg.owner())))
        })
    })
    .await
}

/// v4 the factory's item POST `?action=rename` (`:198-240`) — INFO `Renamed`
/// on every rename, a same-name one included.
pub async fn item_rename(
    db: &Db,
    cfg: ScenarioRouteConfig,
    owner_id: Option<String>,
    scenario_path: String,
    new_filename: String,
    include_archived: bool,
    user_id: String,
) -> Response {
    run(db, cfg, move |main, mount| {
        let attempt = (|| -> Step<Value> {
            let resolved = match resolve_path_or_400(&scenario_path) {
                Ok(p) => p,
                Err(r) => return Ok(Err(r)),
            };
            let location = match load_location(main, mount, &cfg, owner_id.as_deref(), &user_id, true)? {
                Ok(l) => l,
                Err(r) => return Ok(Err(r)),
            };
            let issues = rename_issues(&new_filename);
            if !issues.is_empty() {
                return Ok(Err(invalid_body(issues)));
            }
            let renamed = location.rename(mount, &resolved, &new_filename)?;
            let listed = location.list(mount, include_archived)?;
            owner_line!(info, cfg, owner_id.as_deref(); userId = %user_id, mountPointId = %location.mount_point_id(),
                from = %resolved, to = %renamed.path,
                "{} Renamed {} scenario", cfg.log_tag, cfg.owner());
            let mut body = Map::new();
            body.insert("path".into(), json!(renamed.path));
            body.extend(scenarios_value(&listed));
            Ok(Ok(Value::Object(body)))
        })();
        attempt.unwrap_or_else(|e| {
            if let Some(r) = scenario_error_response(&e) {
                return Err(r);
            }
            owner_line!(error, cfg, owner_id.as_deref(); scenarioPath = %scenario_path, error = %e,
                "{} Failed to rename {} scenario", cfg.log_tag, cfg.owner());
            Err(internal(format!("Failed to rename {} scenario", cfg.owner())))
        })
    })
    .await
}

/// v4 the factory's item DELETE (`:246-282`).
pub async fn item_delete(
    db: &Db,
    cfg: ScenarioRouteConfig,
    owner_id: Option<String>,
    scenario_path: String,
    include_archived: bool,
    user_id: String,
) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    run(db, cfg, move |main, mount| {
        let attempt = (|| -> Step<Value> {
            let location = match load_location(main, mount, &cfg, owner_id.as_deref(), &user_id, true)? {
                Ok(l) => l,
                Err(r) => return Ok(Err(r)),
            };
            location.delete(mount, &resolved)?;
            let listed = location.list(mount, include_archived)?;
            owner_line!(info, cfg, owner_id.as_deref(); userId = %user_id, mountPointId = %location.mount_point_id(),
                relativePath = %resolved,
                "{} Deleted {} scenario", cfg.log_tag, cfg.owner());
            Ok(Ok(Value::Object(scenarios_value(&listed))))
        })();
        attempt.unwrap_or_else(|e| {
            if let Some(r) = scenario_error_response(&e) {
                return Err(r);
            }
            owner_line!(error, cfg, owner_id.as_deref(); scenarioPath = %scenario_path, error = %e,
                "{} Failed to delete {} scenario", cfg.log_tag, cfg.owner());
            Err(internal(format!("Failed to delete {} scenario", cfg.owner())))
        })
    })
    .await
}

/// v4 the collection `list` (`:336-352`): ensure, list,
/// `{ mountPointId, scenarios, warnings }`.
pub async fn collection_list(
    db: &Db,
    cfg: ScenarioRouteConfig,
    owner_id: Option<String>,
    include_archived: bool,
    user_id: String,
) -> Response {
    run(db, cfg, move |main, mount| {
        let attempt = (|| -> Step<Value> {
            let location =
                match ensure_location(main, mount, &cfg, owner_id.as_deref(), &user_id, false)? {
                    Ok(l) => l,
                    Err(r) => return Ok(Err(r)),
                };
            let listed = location.list(mount, include_archived)?;
            let mut body = Map::new();
            body.insert("mountPointId".into(), json!(location.mount_point_id()));
            body.extend(scenarios_value(&listed));
            Ok(Ok(Value::Object(body)))
        })();
        attempt.unwrap_or_else(|e| {
            owner_line!(error, cfg, owner_id.as_deref(); error = %e,
                "{} Failed to list {} scenarios", cfg.log_tag, cfg.owner());
            Err(internal(format!(
                "Failed to list {} scenarios",
                cfg.owner()
            )))
        })
    })
    .await
}

/// v4 the collection `create` (`:354-385`): the body parsed FIRST (Δ13), then
/// ensure, create (conflict 400, a body that parses to nothing → 404 after the
/// write), INFO `Created`, then the re-list with `{includeArchived:
/// validated.archived === true}` (the `d25dacc1` quirk), 201 `{ mountPointId,
/// path, scenarios, warnings }`.
pub async fn collection_create(
    db: &Db,
    cfg: ScenarioRouteConfig,
    owner_id: Option<String>,
    bag: Value,
    user_id: String,
) -> Response {
    run(db, cfg, move |main, mount| {
        let attempt = (|| -> Step<Value> {
            let issues = collect_bag_issues(&bag, true);
            if !issues.is_empty() {
                return Ok(Err(invalid_body(issues)));
            }
            let fields = extract_fields(&bag);
            let location = match ensure_location(main, mount, &cfg, owner_id.as_deref(), &user_id, true)? {
                Ok(l) => l,
                Err(r) => return Ok(Err(r)),
            };
            let scenario = location.create(
                mount,
                &CreateScenarioInput {
                    filename: bag.get("filename").and_then(Value::as_str).unwrap_or("").to_string(),
                    name: fields.name.clone(),
                    description: fields.description.clone(),
                    is_default: fields.is_default,
                    archived: fields.archived,
                    body: fields.body.clone(),
                },
            )?;
            owner_line!(info, cfg, owner_id.as_deref(); userId = %user_id, mountPointId = %location.mount_point_id(),
                relativePath = %scenario.path, isDefault = fields.is_default == Some(true),
                "{} Created {} scenario", cfg.log_tag, cfg.owner());
            let listed = location.list(mount, fields.archived == Some(true))?;
            let mut body = Map::new();
            body.insert("mountPointId".into(), json!(location.mount_point_id()));
            body.insert("path".into(), json!(scenario.path));
            body.extend(scenarios_value(&listed));
            Ok(Ok(Value::Object(body)))
        })();
        attempt.unwrap_or_else(|e| {
            if let Some(r) = scenario_error_response(&e) {
                return Err(r);
            }
            owner_line!(error, cfg, owner_id.as_deref(); error = %e,
                "{} Failed to create {} scenario", cfg.log_tag, cfg.owner());
            Err(internal(format!("Failed to create {} scenario", cfg.owner())))
        })
    })
    .await
}

// ===========================================================================
// The general (instance-wide) family — `/api/v1/scenarios/**`
// ===========================================================================

/// v4 `GET /api/v1/scenarios` (the collection list).
pub async fn scenario_list(db: &Db, include_archived: bool, user_id: &str) -> Response {
    collection_list(db, GENERAL, None, include_archived, user_id.to_string()).await
}

/// v4 `POST /api/v1/scenarios` (the collection create).
pub async fn scenario_create(db: &Db, bag: Value, user_id: &str) -> Response {
    collection_create(db, GENERAL, None, bag, user_id.to_string()).await
}

/// v4 `GET /api/v1/scenarios/[scenarioPath]`.
pub async fn scenario_get(db: &Db, scenario_path: String, user_id: &str) -> Response {
    item_get(db, GENERAL, None, scenario_path, user_id.to_string()).await
}

/// v4 `PUT /api/v1/scenarios/[scenarioPath]`.
pub async fn scenario_update(
    db: &Db,
    scenario_path: String,
    bag: Value,
    include_archived: bool,
    user_id: &str,
) -> Response {
    item_update(
        db,
        GENERAL,
        None,
        scenario_path,
        bag,
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `POST /api/v1/scenarios/[scenarioPath]?action=rename`.
pub async fn scenario_rename(
    db: &Db,
    scenario_path: String,
    new_filename: String,
    include_archived: bool,
    user_id: &str,
) -> Response {
    item_rename(
        db,
        GENERAL,
        None,
        scenario_path,
        new_filename,
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `DELETE /api/v1/scenarios/[scenarioPath]`.
pub async fn scenario_delete(
    db: &Db,
    scenario_path: String,
    include_archived: bool,
    user_id: &str,
) -> Response {
    item_delete(
        db,
        GENERAL,
        None,
        scenario_path,
        include_archived,
        user_id.to_string(),
    )
    .await
}

/// v4 `GET /api/v1/scenarios?action=everywhere` (`scenarios/route.ts:47-55`):
/// `{ sources }`; a failure logs ERROR `[General v1] Failed to list scenarios
/// everywhere` (+ the error) and answers 500 `Failed to list scenarios`.
pub async fn scenario_everywhere(db: &Db, include_archived: bool, user_id: &str) -> Response {
    let user_id = user_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        Ok(
            match list_scenarios_everywhere(main, mount, &user_id, include_archived) {
                Ok(sources) => Ok(json!({ "sources": sources })),
                Err(e) => {
                    tracing::error!(
                        error = %crate::db::fallback::error_text(&e),
                        "[General v1] Failed to list scenarios everywhere"
                    );
                    Err(internal("Failed to list scenarios"))
                }
            },
        )
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Scenario(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// v4 `GET /api/v1/scenarios?action=read&mountPointId=&path=`
/// (`scenarios/route.ts:57-69`): either param falsy → 400 `mountPointId and
/// path are required`; refused → 400 `Invalid scenario ref: <reason>` (NB the
/// spelling differs from the create / action refusal `Invalid scenarioRef:
/// …`); missing → 404 `Scenario not found`; ok → `{ scenario, owner: { scope,
/// id, name, mountPointId } }`. v4 has NO try/catch here — a thrown
/// `URIError` (a malformed `%` in `path`, R-Z) is its middleware's 500.
pub async fn scenario_read_by_ref(
    db: &Db,
    mount_point_id: Option<String>,
    path: Option<String>,
    user_id: &str,
) -> Response {
    let (Some(mount_point_id), Some(path)) = (
        mount_point_id.filter(|m| !m.is_empty()),
        path.filter(|p| !p.is_empty()),
    ) else {
        return bad_request("mountPointId and path are required");
    };
    let user_id = user_id.to_string();
    let out = with_both_conns(db, move |main, mount| {
        let scenario_ref = ScenarioRef {
            mount_point_id,
            path,
        };
        Ok(
            match lookup_scenario_ref(main, mount, &scenario_ref, &user_id) {
                Ok(ScenarioRefLookup::Refused(reason)) => {
                    Err(bad_request(format!("Invalid scenario ref: {reason}")))
                }
                Ok(ScenarioRefLookup::Missing { .. }) => Err(not_found("Scenario")),
                Ok(ScenarioRefLookup::Ok { location, scenario }) => Ok(json!({
                    "scenario": scenario,
                    "owner": {
                        "scope": location.scope().as_str(),
                        "id": location.id(),
                        "name": location.owner_name(),
                        "mountPointId": location.mount_point_id(),
                    },
                })),
                Err(e) => Err(unhandled_route_error(&e)),
            },
        )
    })
    .await;
    match out {
        Ok(Ok(body)) => Response::Scenario(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// A throw v4's route never catches, as its middleware answers it: 500
/// `Internal server error` (measured through the real route — the
/// `general_read_malformed_percent` row of `scenarios_routes_equivalence`).
fn unhandled_route_error(e: &ScenarioLocationError) -> Response {
    tracing::debug!(error = %e, "scenario read-by-ref threw past the route");
    internal("Internal server error")
}
