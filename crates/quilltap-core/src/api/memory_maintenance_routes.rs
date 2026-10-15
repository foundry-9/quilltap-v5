//! The Memories API's maintenance verbs — v4 `app/api/v1/memories/route.ts`
//! (`104e986c7` / `b045dd986`, at the `83334b9a5` pin): POST `?action=maintain`
//! (`:736-817`), GET `?action=maintenance-status` (`:819-833`), POST
//! `?action=repair` (`:835-858`). They fill three of the five C2 wire arms
//! P4.D288 landed with refusal bodies (R-J: every body rides
//! `Response::Memory`); the two `maintenance-config` arms are
//! [`super::memory_maintenance_config`]'s (P4.D288's fill, P4.D297's after).
//!
//! - **`maintain`** validates v4's `maintainSchema` (a malformed body is v5's
//!   memories house class — 400 `Validation error`, message only, P4.D271
//!   R-K; an absent body is `{}`, v4's `req.json().catch(() => ({}))`),
//!   widens `phases` with [`with_implied_phases`] (`consolidate` brings
//!   `mirror`), 404s an unknown character, and then either dry-runs in
//!   process — bounded: ≤ 40 clusters (default 10), a 120 s budget — or
//!   enqueues one job per character (`{jobId: jobIds[0] ?? null, jobIds}`).
//!   "A dry run that may call the model (consolidation) names one
//!   character; the LLM-free phases may dry-run across every character the
//!   user owns."
//! - **R-N / L-5:** the dry run reaches the model through the host-installed
//!   [`maintenance_runner`]; an engine without one answers the loud typed
//!   refusal AFTER v4's 400 / 404 arms — even for phases that call no model
//!   (v4 runs them; the difference is reachable only in a host-less engine).

use serde_json::{json, Map, Value};

use crate::api::types::{ErrorKind, Response};
use crate::db::characters_read;
use crate::db::instance_settings::{
    parse_memory_consolidation_patch, MEMORY_MAINTENANCE_CAP_MAX, MEMORY_MAINTENANCE_CAP_MIN,
};
use crate::db::runtime::Db;
use crate::services::memory_maintenance::ledger::get_maintenance_ledger_status;
use crate::services::memory_maintenance::runner::maintenance_runner;
use crate::services::memory_maintenance::types::{
    with_implied_phases, MaintenanceContext, MaintenanceMode, MaintenancePhase, MaintenanceReason,
    MaintenanceRequest, MaintenanceRunSettings,
};
use crate::services::memory_repair::repair_memory_graph;
use crate::services::queue_service::{
    enqueue_memory_maintenance, enqueue_memory_maintenance_for_user,
};

/// v4 `MAINTAIN_DRY_RUN_DEFAULT_CLUSTERS`: "in-process dry runs are bounded:
/// each consolidation cluster is one model call."
pub const MAINTAIN_DRY_RUN_DEFAULT_CLUSTERS: f64 = 10.0;
/// v4 `MAINTAIN_DRY_RUN_MAX_CLUSTERS`.
pub const MAINTAIN_DRY_RUN_MAX_CLUSTERS: f64 = 40.0;
/// v4 `MAINTAIN_DRY_RUN_TIME_BUDGET_MS`.
pub const MAINTAIN_DRY_RUN_TIME_BUDGET_MS: i64 = 120_000;

/// R-N's refusal text (the P4.D271 R-E precedent).
pub const MAINTENANCE_RUNNER_MISSING: &str =
    "The maintenance dry run needs the host's maintenance runner, which is not installed in this engine.";

fn validation_error() -> Response {
    Response::error(ErrorKind::BadRequest, "Validation error")
}

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// `z.number().int().min(lo).max(hi)` — pass / fail.
fn int_in(v: &Value, lo: f64, hi: f64) -> Option<Value> {
    let n = v.as_f64()?;
    (n.fract() == 0.0 && n >= lo && n <= hi).then(|| crate::db::js_number_to_json(n))
}

/// v4 `maintainSchema`'s parsed output.
struct MaintainBody {
    character_id: Option<String>,
    phases: Option<Vec<MaintenancePhase>>,
    dry_run: Option<bool>,
    /// The parsed `settings` object (schema order: `consolidation`,
    /// `maxClustersPerRun`, `cap`; unknown keys stripped).
    settings: Option<Map<String, Value>>,
}

/// v4 `maintainSchema.safeParse(body)` (`route.ts:129-141`); `None` = invalid.
fn parse_maintain(body: &Value) -> Option<MaintainBody> {
    let empty = Map::new();
    let obj = match body {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => return None,
    };
    let character_id = match obj.get("characterId") {
        None => None,
        Some(Value::String(s)) if crate::api::zod_issues::zod_uuid_ok(s) => Some(s.clone()),
        Some(_) => return None,
    };
    let phases = match obj.get("phases") {
        None => None,
        Some(Value::Array(list)) if !list.is_empty() => {
            let mut out = Vec::new();
            for p in list {
                out.push(p.as_str().and_then(MaintenancePhase::parse)?);
            }
            Some(out)
        }
        Some(_) => return None,
    };
    let dry_run = match obj.get("dryRun") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return None,
    };
    let settings = match obj.get("settings") {
        None => None,
        Some(Value::Object(s)) => {
            let mut out = Map::new();
            if let Some(c) = s.get("consolidation") {
                let patch = parse_memory_consolidation_patch(c).ok()?;
                out.insert("consolidation".into(), Value::Object(patch));
            }
            if let Some(m) = s.get("maxClustersPerRun") {
                out.insert("maxClustersPerRun".into(), int_in(m, 1.0, 1000.0)?);
            }
            if let Some(c) = s.get("cap") {
                out.insert(
                    "cap".into(),
                    int_in(
                        c,
                        MEMORY_MAINTENANCE_CAP_MIN as f64,
                        MEMORY_MAINTENANCE_CAP_MAX as f64,
                    )?,
                );
            }
            Some(out)
        }
        Some(_) => return None,
    };
    Some(MaintainBody {
        character_id,
        phases,
        dry_run,
        settings,
    })
}

/// The overlay character read (v4 `repos.characters.findById`).
fn find_character(db: &Db, id: &str) -> Result<Option<Value>, crate::db::DbError> {
    let id = id.to_string();
    db.read_main(|main| db.read_mount_index(|mount| characters_read::find_by_id(main, mount, &id)))
}

fn phase_list_json(phases: &[MaintenancePhase]) -> String {
    serde_json::to_string(&phases.iter().map(|p| p.as_str()).collect::<Vec<_>>())
        .unwrap_or_default()
}

/// v4 POST `?action=maintain` (`handleMaintain`).
pub async fn memory_maintain(
    db: &Db,
    user_id: &str,
    options: Value,
    host_zone_fallback: &str,
) -> Response {
    let Some(body) = parse_maintain(&options) else {
        return validation_error();
    };
    let phases = body.phases.as_deref().map(with_implied_phases);

    if let Some(cid) = &body.character_id {
        match find_character(db, cid) {
            Ok(Some(_)) => {}
            Ok(None) => return Response::error(ErrorKind::NotFound, "Character not found"),
            Err(e) => return internal(e),
        }
    }

    if body.dry_run == Some(true) {
        // "A dry run that may call the model (consolidation) names one
        // character; the LLM-free phases may dry-run across every character
        // the user owns."
        let may_call_model = phases
            .as_ref()
            .is_none_or(|p| p.contains(&MaintenancePhase::Consolidate));
        if body.character_id.is_none() && may_call_model {
            return Response::error(
                ErrorKind::BadRequest,
                "A dry run that includes consolidation names one character (characterId)",
            );
        }
        let max_clusters = body
            .settings
            .as_ref()
            .and_then(|s| s.get("maxClustersPerRun"))
            .and_then(Value::as_f64)
            .unwrap_or(MAINTAIN_DRY_RUN_DEFAULT_CLUSTERS)
            .min(MAINTAIN_DRY_RUN_MAX_CLUSTERS);
        let targets: Vec<String> = match &body.character_id {
            Some(cid) => vec![cid.clone()],
            None => {
                let uid = user_id.to_string();
                let read = db.read_main(|main| {
                    db.read_mount_index(|mount| characters_read::find_by_user_id(main, mount, &uid))
                });
                match read {
                    Ok(list) => list
                        .iter()
                        .filter(|c| !crate::api::characters::is_archived(c))
                        .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect(),
                    Err(e) => return internal(e),
                }
            }
        };
        let character_text = body.character_id.as_deref().unwrap_or("all");
        match &phases {
            Some(p) => tracing::debug!(
                characterId = %character_text,
                characters = targets.len(),
                phasesJson = %phase_list_json(p),
                maxClustersPerRun = %crate::services::memory_maintenance::js_num(max_clusters),
                "[Memories API] Maintenance dry run (in-process)"
            ),
            None => tracing::debug!(
                characterId = %character_text,
                characters = targets.len(),
                phases = "all",
                maxClustersPerRun = %crate::services::memory_maintenance::js_num(max_clusters),
                "[Memories API] Maintenance dry run (in-process)"
            ),
        }
        let Some(runner) = maintenance_runner() else {
            return Response::error(ErrorKind::Internal, MAINTENANCE_RUNNER_MISSING);
        };
        // `settings: { ...settings, maxClustersPerRun }`.
        let mut run_settings = body.settings.clone().unwrap_or_default();
        run_settings.insert(
            "maxClustersPerRun".into(),
            crate::db::js_number_to_json(max_clusters),
        );
        let run_settings = MaintenanceRunSettings::from_value(&Value::Object(run_settings));
        let mut reports: Vec<Value> = Vec::new();
        for target in &targets {
            let request = MaintenanceRequest {
                character_id: target.clone(),
                phases: phases
                    .as_ref()
                    .map(|p| p.iter().map(|x| x.as_str().to_string()).collect()),
                reason: MaintenanceReason::Manual,
                exempt_phases: None,
                dry_run: true,
                settings: run_settings.clone(),
            };
            let ctx = MaintenanceContext {
                user_id: Some(user_id.to_string()),
                time_budget_ms: Some(MAINTAIN_DRY_RUN_TIME_BUDGET_MS),
                now_ms: None,
                mode: MaintenanceMode::InProcess,
                elapsed: None,
                host_zone_fallback: host_zone_fallback.to_string(),
            };
            match runner.run(db, request, ctx).await {
                Ok(run) => reports.push(run.report.to_json()),
                Err(e) => {
                    match &body.character_id {
                        Some(cid) => tracing::error!(
                            characterId = %cid,
                            error = %crate::db::fallback::error_text(&e),
                            "[Memories API] Maintenance dry run failed"
                        ),
                        None => tracing::error!(
                            error = %crate::db::fallback::error_text(&e),
                            "[Memories API] Maintenance dry run failed"
                        ),
                    }
                    return Response::error(ErrorKind::Internal, "Maintenance dry run failed");
                }
            }
        }
        return Response::Memory(if body.character_id.is_some() {
            json!({
                "success": true,
                "dryRun": true,
                "report": reports.into_iter().next().unwrap_or(Value::Null),
            })
        } else {
            json!({ "success": true, "dryRun": true, "reports": reports })
        });
    }

    // `{ reason: 'manual', ...(phases ? {phases} : {}), ...(settings ? {settings} : {}) }`.
    let mut payload = Map::new();
    payload.insert("reason".into(), json!("manual"));
    if let Some(p) = &phases {
        payload.insert(
            "phases".into(),
            json!(p.iter().map(|x| x.as_str()).collect::<Vec<_>>()),
        );
    }
    if let Some(s) = &body.settings {
        payload.insert("settings".into(), Value::Object(s.clone()));
    }
    let enqueued = match &body.character_id {
        Some(cid) => {
            let mut one = payload.clone();
            one.insert("characterId".into(), json!(cid));
            enqueue_memory_maintenance(db, user_id, one, None)
                .await
                .map(|id| vec![id])
        }
        None => enqueue_memory_maintenance_for_user(db, user_id, payload).await,
    };
    match enqueued {
        Ok(job_ids) => {
            let character_text = body.character_id.as_deref().unwrap_or("all");
            match &phases {
                Some(p) => tracing::info!(
                    userId = %user_id,
                    characterId = %character_text,
                    phasesJson = %phase_list_json(p),
                    jobs = job_ids.len(),
                    "[Memories API] Maintenance enqueued"
                ),
                None => tracing::info!(
                    userId = %user_id,
                    characterId = %character_text,
                    phases = "all",
                    jobs = job_ids.len(),
                    "[Memories API] Maintenance enqueued"
                ),
            }
            Response::Memory(json!({
                "success": true,
                "dryRun": false,
                "jobId": job_ids.first().cloned(),
                "jobIds": job_ids,
            }))
        }
        Err(e) => {
            match &body.character_id {
                Some(cid) => tracing::error!(
                    characterId = %cid,
                    error = %crate::db::fallback::error_text(&e),
                    "[Memories API] Failed to enqueue maintenance"
                ),
                None => tracing::error!(
                    error = %crate::db::fallback::error_text(&e),
                    "[Memories API] Failed to enqueue maintenance"
                ),
            }
            Response::error(ErrorKind::Internal, "Failed to enqueue maintenance")
        }
    }
}

/// v4 GET `?action=maintenance-status` (`handleMaintenanceStatus`): a named
/// character that does not exist → 404; `Some("")` (present-empty) reads as
/// every entry (JS-falsy); DEBUG `[Memories API] Maintenance status read
/// {characterId: id ?? 'all', entries}` (`??` — the empty string is logged as
/// itself); `{success, ledger}`.
pub fn memory_maintenance_status(db: &Db, character_id: Option<&str>) -> Response {
    if let Some(cid) = character_id.filter(|c| !c.is_empty()) {
        match find_character(db, cid) {
            Ok(Some(_)) => {}
            Ok(None) => return Response::error(ErrorKind::NotFound, "Character not found"),
            Err(e) => return internal(e),
        }
    }
    let ledger: Vec<Value> = get_maintenance_ledger_status(db, character_id)
        .iter()
        .map(|s| s.to_json())
        .collect();
    tracing::debug!(
        characterId = %character_id.unwrap_or("all"),
        entries = ledger.len(),
        "[Memories API] Maintenance status read"
    );
    Response::Memory(json!({ "success": true, "ledger": ledger }))
}

/// v4 POST `?action=repair` (`handleRepair`): `repairSchema` (`characterId:
/// z.uuid().optional(), dryRun: z.boolean().optional()`; absent body `{}`),
/// `dryRun = false` by default, a named character that does not exist → 404,
/// `{success, report}`; a failed scan → ERROR `[Memories API] Memory graph
/// repair failed {characterId}` + 500 `Memory graph repair failed`.
pub async fn memory_repair(db: &Db, user_id: &str, options: Value) -> Response {
    let empty = Map::new();
    let obj = match &options {
        Value::Null => &empty,
        Value::Object(o) => o,
        _ => return validation_error(),
    };
    let character_id = match obj.get("characterId") {
        None => None,
        Some(Value::String(s)) if crate::api::zod_issues::zod_uuid_ok(s) => Some(s.clone()),
        Some(_) => return validation_error(),
    };
    let dry_run = match obj.get("dryRun") {
        None => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return validation_error(),
    };
    if let Some(cid) = &character_id {
        match find_character(db, cid) {
            Ok(Some(_)) => {}
            Ok(None) => return Response::error(ErrorKind::NotFound, "Character not found"),
            Err(e) => return internal(e),
        }
    }
    match repair_memory_graph(db, user_id, character_id.as_deref(), dry_run).await {
        Ok(report) => Response::Memory(json!({ "success": true, "report": report.to_json() })),
        Err(e) => {
            match &character_id {
                Some(cid) => tracing::error!(
                    characterId = %cid,
                    error = %crate::db::fallback::error_text(&e),
                    "[Memories API] Memory graph repair failed"
                ),
                None => tracing::error!(
                    error = %crate::db::fallback::error_text(&e),
                    "[Memories API] Memory graph repair failed"
                ),
            }
            Response::error(ErrorKind::Internal, "Memory graph repair failed")
        }
    }
}
