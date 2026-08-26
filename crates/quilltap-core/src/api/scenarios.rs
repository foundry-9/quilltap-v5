//! Shared scenario route-logic + the general (instance-wide) scenarios family
//! (P4.6n) — the mount-scoped CRUD every scope's routes compose, plus the
//! `/api/v1/scenarios` handlers that resolve the singleton "Quilltap General"
//! store (with the pre-provision race arms).
//!
//! The scope-specific store resolution lives in each family's dispatch module
//! (`api::groups` / `api::projects` for the `[id]` scopes; the general handlers
//! below for the instance-wide scope). Everything downstream of a resolved
//! `(mount_conn, mount_point_id)` — parse/validate the scenario bag, the
//! create-collision guard, the write + set-default, the fresh re-list — is shared
//! here so the three families stay byte-identical (v4's `scenarios-common.ts` + the
//! Zod schemas duplicated across the route files).
//!
//! Each op returns `Result<Result<Value, Response>, ScenarioWriteError>`: the
//! inner `Ok(Value)` is the success body, the inner `Err(Response)` is an early
//! 400/404, and the outer error is a store/DB failure the caller maps to Internal.

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::db::database_store::{
    delete_database_document, move_database_document, write_database_document,
};
use crate::db::doc_mount_documents::DocMountDocumentsRepository;
use crate::db::doc_mount_file_links::DocMountFileLinksRepository;
use crate::db::instance_settings::get_general_mount_point_id;
use crate::db::runtime::Db;
use crate::db::scenarios::{
    build_scenario_file_content, is_scenario_content_archived, list_scenarios_in_folder,
    read_scenario_by_path, resolve_scenario_path, set_scenario_default_in_folder,
    ListScenariosResult, ScenarioPathResolution, ScenarioWriteError, SCENARIOS_FOLDER,
};
use crate::db::DbError;
use crate::jsstr::utf16_len;
use crate::services::image_job_common::with_both_conns;
use crate::vault_overlay::sanitize_file_name;

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

/// Map a scenario write/DB failure to an Internal response (the family routes'
/// outer catch → serverError).
pub(crate) fn write_err(e: ScenarioWriteError) -> Response {
    internal(e)
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
            Some(Value::String(s)) if (1..=100).contains(&utf16_len(s)) => {}
            Some(Value::String(_)) => issues.push("Invalid filename length".to_string()),
            _ => issues.push("Required".to_string()),
        }
    }
    // name?: string 1..=200 when present.
    if let Some(v) = obj.get("name") {
        if !v.is_null() {
            match v {
                Value::String(s) if (1..=200).contains(&utf16_len(s)) => {}
                _ => issues.push("Invalid name".to_string()),
            }
        }
    }
    // description?: string ≤500 when present.
    if let Some(v) = obj.get("description") {
        if !v.is_null() {
            match v {
                Value::String(s) if utf16_len(s) <= 500 => {}
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
    // archived?: boolean when present (v4 `d25dacc1`).
    if let Some(v) = obj.get("archived") {
        if !v.is_null() && !v.is_boolean() {
            issues.push("Invalid archived".to_string());
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

/// v4 `doc.fileName.replace(/\.md$/i, '')` on the sanitised filename.
fn strip_md(name: &str) -> String {
    let n = name.len();
    if n >= 3
        && name.as_bytes()[n - 3] == b'.'
        && name.as_bytes()[n - 2].eq_ignore_ascii_case(&b'm')
        && name.as_bytes()[n - 1].eq_ignore_ascii_case(&b'd')
    {
        name[..n - 3].to_string()
    } else {
        name.to_string()
    }
}

fn scenarios_value(listed: &ListScenariosResult) -> Value {
    json!({
        "scenarios": serde_json::to_value(&listed.scenarios).unwrap_or_else(|_| json!([])),
        "warnings": listed.warnings,
    })
}

// ===========================================================================
// Shared mount-scoped CRUD ops
// ===========================================================================

/// list body `{ mountPointId, scenarios, warnings }` (v4 the collection GET).
pub(crate) fn list_body(
    mount: &Connection,
    mp: &str,
    include_archived: bool,
) -> Result<Value, DbError> {
    let listed = list_scenarios_in_folder(mount, mp, SCENARIOS_FOLDER, include_archived)?;
    let mut body = scenarios_value(&listed);
    body.as_object_mut()
        .unwrap()
        .insert("mountPointId".into(), json!(mp));
    Ok(body)
}

/// create → `{ mountPointId, path, scenarios, warnings }` (201). Handles filename
/// sanitisation, the empty-after-sanitise 400, the collision 400, the write, and
/// the isDefault demotion. v4's collection POST create.
pub(crate) fn create_op(
    mount: &Connection,
    mp: &str,
    bag: &Value,
) -> Result<Result<Value, Response>, ScenarioWriteError> {
    let issues = collect_bag_issues(bag, true);
    if !issues.is_empty() {
        return Ok(Err(bad_request(format!(
            "Invalid request body: {}",
            issues.join("; ")
        ))));
    }
    let filename = bag
        .get("filename")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let fields = extract_fields(bag);

    let cleaned = strip_md(&sanitize_file_name(&filename));
    if cleaned.is_empty() {
        return Ok(Err(bad_request(
            "Filename cannot be empty after sanitisation",
        )));
    }
    let relative_path = format!("{SCENARIOS_FOLDER}/{cleaned}.md");

    let docs = DocMountDocumentsRepository::new(mount);
    if docs
        .find_by_mount_point_and_path(mp, &relative_path)?
        .is_some()
    {
        return Ok(Err(bad_request(format!(
            "A scenario named \"{cleaned}\" already exists"
        ))));
    }

    let content = build_scenario_file_content(
        fields.name.as_deref(),
        fields.description.as_deref(),
        fields.is_default.unwrap_or(false),
        fields.archived.unwrap_or(false),
        &fields.body,
    );
    write_database_document(mount, mp, &relative_path, &content)?;
    if fields.is_default == Some(true) {
        set_scenario_default_in_folder(mount, mp, &relative_path, SCENARIOS_FOLDER)?;
    }

    // ⚠ v4 `d25dacc1` quirk, reproduced: the three file-backed collection POSTs
    // refresh their returned list with `{includeArchived: validated.archived ===
    // true}` — the BODY, not the query param. So creating an ACTIVE scenario
    // with "Show archived" ticked returns an archived-free list.
    let listed =
        list_scenarios_in_folder(mount, mp, SCENARIOS_FOLDER, fields.archived == Some(true))?;
    let mut body = scenarios_value(&listed);
    let obj = body.as_object_mut().unwrap();
    obj.insert("mountPointId".into(), json!(mp));
    obj.insert("path".into(), json!(relative_path));
    Ok(Ok(body))
}

/// get one scenario → `{ scenario }`; 400 on a bad path, 404 when missing.
pub(crate) fn get_op(
    mount: &Connection,
    mp: &str,
    resolved_path: &str,
) -> Result<Result<Value, Response>, ScenarioWriteError> {
    match read_scenario_by_path(mount, mp, resolved_path)? {
        Some(s) => Ok(Ok(
            json!({ "scenario": serde_json::to_value(&s).unwrap_or(Value::Null) }),
        )),
        None => Ok(Err(not_found("Scenario"))),
    }
}

/// update → `{ scenarios, warnings }`; 400 ZodError, 404 when missing.
pub(crate) fn update_op(
    mount: &Connection,
    mp: &str,
    resolved_path: &str,
    bag: &Value,
    include_archived: bool,
) -> Result<Result<Value, Response>, ScenarioWriteError> {
    let issues = collect_bag_issues(bag, false);
    if !issues.is_empty() {
        return Ok(Err(bad_request(format!(
            "Invalid request body: {}",
            issues.join("; ")
        ))));
    }
    let fields = extract_fields(bag);

    let docs = DocMountDocumentsRepository::new(mount);
    let Some(existing) = docs.find_by_mount_point_and_path(mp, resolved_path)? else {
        return Ok(Err(not_found("Scenario")));
    };

    let content = build_scenario_file_content(
        fields.name.as_deref(),
        fields.description.as_deref(),
        fields.is_default.unwrap_or(false),
        // v4: `validated.archived ?? isScenarioContentArchived(existing.content)`
        // — omitting the key PRESERVES the flag through the whole-file rewrite.
        fields
            .archived
            .unwrap_or_else(|| is_scenario_content_archived(&existing)),
        &fields.body,
    );
    write_database_document(mount, mp, resolved_path, &content)?;
    if fields.is_default == Some(true) {
        set_scenario_default_in_folder(mount, mp, resolved_path, SCENARIOS_FOLDER)?;
    }

    let listed = list_scenarios_in_folder(mount, mp, SCENARIOS_FOLDER, include_archived)?;
    Ok(Ok(scenarios_value(&listed)))
}

/// rename → `{ path, scenarios, warnings }`; no-op same-path, 404 source missing,
/// 400 target exists.
pub(crate) fn rename_op(
    mount: &Connection,
    mp: &str,
    resolved_path: &str,
    new_filename: &str,
    include_archived: bool,
) -> Result<Result<Value, Response>, ScenarioWriteError> {
    let cleaned = strip_md(&sanitize_file_name(new_filename));
    if cleaned.is_empty() {
        return Ok(Err(bad_request(
            "newFilename cannot be empty after sanitisation",
        )));
    }
    let new_path = format!("{SCENARIOS_FOLDER}/{cleaned}.md");

    let docs = DocMountDocumentsRepository::new(mount);
    if new_path == resolved_path {
        // No-op rename — return current state.
        let listed = list_scenarios_in_folder(mount, mp, SCENARIOS_FOLDER, include_archived)?;
        let mut body = scenarios_value(&listed);
        body.as_object_mut()
            .unwrap()
            .insert("path".into(), json!(new_path));
        return Ok(Ok(body));
    }
    if docs
        .find_by_mount_point_and_path(mp, resolved_path)?
        .is_none()
    {
        return Ok(Err(not_found("Scenario")));
    }
    if docs.find_by_mount_point_and_path(mp, &new_path)?.is_some() {
        return Ok(Err(bad_request(format!(
            "A scenario named \"{cleaned}\" already exists"
        ))));
    }

    move_database_document(mount, mp, resolved_path, &new_path)?;

    let listed = list_scenarios_in_folder(mount, mp, SCENARIOS_FOLDER, include_archived)?;
    let mut body = scenarios_value(&listed);
    body.as_object_mut()
        .unwrap()
        .insert("path".into(), json!(new_path));
    Ok(Ok(body))
}

/// delete → `{ scenarios, warnings }`; 404 when nothing was deleted.
pub(crate) fn delete_op(
    mount: &Connection,
    mp: &str,
    resolved_path: &str,
    include_archived: bool,
) -> Result<Result<Value, Response>, ScenarioWriteError> {
    let deleted = delete_database_document(mount, mp, resolved_path)?;
    if !deleted {
        return Ok(Err(not_found("Scenario")));
    }
    let listed = list_scenarios_in_folder(mount, mp, SCENARIOS_FOLDER, include_archived)?;
    Ok(Ok(scenarios_value(&listed)))
}

/// Resolve a scenario path, or the 400 rejection Response.
pub(crate) fn resolve_path_or_400(scenario_path: &str) -> Result<String, Response> {
    match resolve_scenario_path(scenario_path, SCENARIOS_FOLDER) {
        ScenarioPathResolution::Ok(p) => Ok(p),
        ScenarioPathResolution::Err(e) => Err(bad_request(e)),
    }
}

/// Ensure the `Scenarios/` folder in a resolved store (idempotent; best-effort,
/// matching v4's non-fatal ensure).
pub(crate) fn ensure_scenarios_folder(mount: &Connection, mp: &str) {
    let _ = DocMountFileLinksRepository::new(mount).ensure_folder_path(mp, SCENARIOS_FOLDER);
}

// ===========================================================================
// The general (instance-wide) family — resolve the singleton store + race arms
// ===========================================================================

/// v4 `ensureGeneralScenariosFolder`/`getGeneralMountPointId` — the general mount
/// id, or `None` when the "Quilltap General" store has not been provisioned.
fn general_mount_id(main: &Connection, mount: &Connection) -> Result<Option<String>, DbError> {
    let Some(mp) = get_general_mount_point_id(main)? else {
        return Ok(None);
    };
    ensure_scenarios_folder(mount, &mp);
    Ok(Some(mp))
}

/// Finish a general-family op result into a Response.
fn finish(out: Result<Result<Value, Response>, DbError>) -> Response {
    match out {
        Ok(Ok(body)) => Response::Scenario(body),
        Ok(Err(r)) => r,
        Err(e) => internal(e),
    }
}

/// Map a shared op's `Result<Result<Value, Response>, ScenarioWriteError>` into
/// the closure's `Result<Result<Value, Response>, DbError>` (write failures →
/// an inner Internal response).
fn lift(
    op: Result<Result<Value, Response>, ScenarioWriteError>,
) -> Result<Result<Value, Response>, DbError> {
    Ok(match op {
        Ok(inner) => inner,
        Err(e) => Err(write_err(e)),
    })
}

/// v4 `GET /api/v1/scenarios`: the pre-provision race returns
/// `{ mountPointId: null, scenarios: [], warnings: [] }`; otherwise the list.
pub async fn scenario_list(db: &Db, include_archived: bool) -> Response {
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = general_mount_id(main, mount)? else {
            return Ok(Ok(
                json!({ "mountPointId": null, "scenarios": [], "warnings": [] }),
            ));
        };
        Ok(Ok(list_body(mount, &mp, include_archived)?))
    })
    .await;
    finish(out)
}

/// v4 `POST /api/v1/scenarios`: the pre-provision race is a 400
/// (`Quilltap General mount has not been provisioned yet — restart the server`).
pub async fn scenario_create(db: &Db, bag: Value) -> Response {
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = general_mount_id(main, mount)? else {
            return Ok(Err(bad_request(
                "Quilltap General mount has not been provisioned yet — restart the server",
            )));
        };
        lift(create_op(mount, &mp, &bag))
    })
    .await;
    finish(out)
}

/// v4 `GET /api/v1/scenarios/[scenarioPath]`.
pub async fn scenario_get(db: &Db, scenario_path: String) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = general_mount_id(main, mount)? else {
            return Ok(Err(not_found(
                "Quilltap General mount has not been provisioned yet",
            )));
        };
        lift(get_op(mount, &mp, &resolved))
    })
    .await;
    finish(out)
}

/// v4 `PUT /api/v1/scenarios/[scenarioPath]`.
pub async fn scenario_update(
    db: &Db,
    scenario_path: String,
    bag: Value,
    include_archived: bool,
) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = general_mount_id(main, mount)? else {
            return Ok(Err(not_found(
                "Quilltap General mount has not been provisioned yet",
            )));
        };
        lift(update_op(mount, &mp, &resolved, &bag, include_archived))
    })
    .await;
    finish(out)
}

/// v4 `POST /api/v1/scenarios/[scenarioPath]?action=rename`.
pub async fn scenario_rename(
    db: &Db,
    scenario_path: String,
    new_filename: String,
    include_archived: bool,
) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = general_mount_id(main, mount)? else {
            return Ok(Err(not_found(
                "Quilltap General mount has not been provisioned yet",
            )));
        };
        lift(rename_op(
            mount,
            &mp,
            &resolved,
            &new_filename,
            include_archived,
        ))
    })
    .await;
    finish(out)
}

/// v4 `DELETE /api/v1/scenarios/[scenarioPath]`.
pub async fn scenario_delete(db: &Db, scenario_path: String, include_archived: bool) -> Response {
    let resolved = match resolve_path_or_400(&scenario_path) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let out = with_both_conns(db, move |main, mount| {
        let Some(mp) = general_mount_id(main, mount)? else {
            return Ok(Err(not_found(
                "Quilltap General mount has not been provisioned yet",
            )));
        };
        lift(delete_op(mount, &mp, &resolved, include_archived))
    })
    .await;
    finish(out)
}
