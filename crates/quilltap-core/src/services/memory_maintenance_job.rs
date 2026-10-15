//! The `MEMORY_MAINTENANCE` job handler — v4
//! `lib/background-jobs/handlers/memory-maintenance.ts` (`104e986c7`, at the
//! `83334b9a5` pin).
//!
//! v4's why, carried: "Runs `runMaintenance` for one character … fold
//! near-duplicates → consolidate → demote → retire → mirror, each skippable
//! by settings. … Every effect waits for the commit: the dispatcher's hooks
//! invalidate the frozen archives, publish `memories`, write the
//! `Commonplace/` mirror from the committed rows (bug 208) and record the
//! summary this handler hands back in the parent's ledger. A consolidation
//! model call lost to a timeout does not fail the job — that would discard
//! every other cluster's digests with it; its rows stay unconsidered and the
//! next run retries them."
//!
//! v5: the run is [`MaintenanceMode::Job`] (R-M — it reports as v4's child
//! does); the summary is STASHED for the completion region
//! ([`crate::services::memory_maintenance::job_results`]) and the handler
//! answers `Completed(None)` — never `Completed(Some(_))`, which would merge
//! it into the row's `payload.result`. R-AL: v4's
//! `QUILLTAP_TEST_MAINTENANCE_FAIL_AFTER_PLAN` flag (and its two arms) is
//! NO-PORT (ratified by P4.D303) — v5 has no buffered batch to drop.

use std::sync::Arc;

use serde_json::Value;

use crate::db::background_jobs::{BackgroundJob, BackgroundJobsRepository};
use crate::db::runtime::Db;
use crate::episodic::js_date_parse_ms;
use crate::services::job_runner::{JobFuture, JobHandler, JobOutcome};
use crate::services::memory_maintenance::job_results::{stash, MaintenanceJobResult};
use crate::services::memory_maintenance::run::summarize_maintenance;
use crate::services::memory_maintenance::runner::MaintenanceRunner;
use crate::services::memory_maintenance::types::{
    MaintenanceContext, MaintenanceMode, MaintenanceReason, MaintenanceRequest,
    MaintenanceRunSettings,
};
use crate::services::queue_service::{enqueue_memory_maintenance, MEMORY_MAINTENANCE};

/// v4 `MAINTENANCE_JOB_TIME_BUDGET_MS`: "wall-clock budget for one job's
/// consolidation calls. The dispatcher marks a job PROCESSING for more than
/// ten minutes as stuck; a run stops issuing calls after this long and commits
/// what it has."
pub const MAINTENANCE_JOB_TIME_BUDGET_MS: i64 = 7 * 60 * 1000;

/// v4 `MAINTENANCE_DEFER_MS`: "how far a job deferred behind a running one is
/// pushed back".
pub const MAINTENANCE_DEFER_MS: i64 = 2 * 60 * 1000;

const MODULE: &str = "jobs:memory-maintenance";

/// v4 `enqueuedBefore(a, b)`: "true when `a` was enqueued before `b` (ties
/// broken by id), so `b` yields to it".
fn enqueued_before(a: &BackgroundJob, b: &BackgroundJob) -> bool {
    let at = js_date_parse_ms(&a.created_at);
    let bt = js_date_parse_ms(&b.created_at);
    if let (Some(at), Some(bt)) = (at, bt) {
        if at != bt {
            return at < bt;
        }
    }
    // `a.id < b.id` — a UTF-16 code-unit compare (ids are ASCII).
    a.id < b.id
}

/// v4 `findOtherRunningMaintenance(job)`: "the id of an older PROCESSING
/// maintenance job for this job's character, if any. Only the younger of two
/// concurrent runs yields, so two jobs that start together never both defer."
/// v4's catch (WARN `Could not check for a concurrent maintenance run;
/// running anyway {jobId, error}`) is unreachable through v4's real code —
/// `findByUserId` is a fallback `safeQuery` (`Error finding entities by
/// filter`, `[]`): a failed read finds no running job. Pinned negative.
fn find_other_running_maintenance(
    db: &Db,
    job: &BackgroundJob,
    character_id: &Value,
) -> Option<String> {
    let uid = job.user_id.clone();
    let processing = db
        .read_main(|c| {
            Ok(crate::db::fallback::find_by_filter_or_empty(
                "background_jobs",
                || BackgroundJobsRepository::new(c).find_by_user_id(&uid, Some("PROCESSING")),
            ))
        })
        .unwrap_or_default();
    processing
        .iter()
        .find(|j| {
            j.id != job.id
                && j.job_type == MEMORY_MAINTENANCE
                && serde_json::from_str::<Value>(&j.payload)
                    .ok()
                    .and_then(|p| p.get("characterId").cloned())
                    .as_ref()
                    == Some(character_id)
                && enqueued_before(j, job)
        })
        .map(|j| j.id.clone())
}

fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    v.and_then(Value::as_array).map(|a| {
        a.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    })
}

/// JS truthiness of an optional JSON value (`...(payload.x ? {x} : {})`).
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

fn null_or(v: Value) -> String {
    match v {
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

/// v4 `handleMemoryMaintenance(job)`.
pub async fn handle_memory_maintenance(
    db: &Db,
    runner: &dyn MaintenanceRunner,
    job: &BackgroundJob,
    host_zone_fallback: &str,
) -> JobOutcome {
    let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
    // `!payload?.characterId` — JS-falsy (absent, null, empty, 0, false).
    let character_value = payload.get("characterId").cloned();
    if !truthy(character_value.as_ref()) {
        tracing::warn!(
            module = MODULE,
            jobId = %job.id,
            "Maintenance job has no characterId; nothing to do"
        );
        return JobOutcome::Completed(None);
    }
    let character_value = character_value.expect("truthy above");
    let character_id = character_value
        .as_str()
        .map_or_else(|| character_value.to_string(), str::to_string);

    // "One run per character at a time (bug 221). A running job never absorbs
    // a new request (it has already read the corpus), so this one may have
    // been enqueued beside it: hand it back to the queue for later and stand
    // down."
    if let Some(running) = find_other_running_maintenance(db, job, &character_value) {
        let scheduled_at =
            crate::clock::iso_from_unix_ms(crate::clock::now_unix_ms() + MAINTENANCE_DEFER_MS);
        let own = match &payload {
            Value::Object(m) => m.clone(),
            _ => serde_json::Map::new(),
        };
        return match enqueue_memory_maintenance(db, &job.user_id, own, Some(scheduled_at)).await {
            Ok(successor_id) => {
                tracing::info!(
                    module = MODULE,
                    jobId = %job.id,
                    characterId = %character_id,
                    runningJobId = %running,
                    successorId = %successor_id,
                    "Another maintenance run is under way for the character; deferred"
                );
                JobOutcome::Completed(None)
            }
            Err(e) => JobOutcome::Failed(crate::db::fallback::error_text(&e)),
        };
    }

    // `reason: payload.reason ?? 'manual'`. v4 carries an unrecognized string
    // as is; the typed run reports it as manual, and — as v4's `reason !==
    // 'manual'` would — exempts nothing for it (every stored payload carries
    // `exemptPhases` anyway: the enqueue normalizes it).
    let raw_reason = payload.get("reason").filter(|r| !r.is_null());
    let parsed_reason = match raw_reason {
        None => Some(MaintenanceReason::Manual),
        Some(r) => r.as_str().and_then(MaintenanceReason::parse),
    };
    let reason = parsed_reason.unwrap_or(MaintenanceReason::Manual);
    let request = MaintenanceRequest {
        character_id: character_id.clone(),
        phases: if truthy(payload.get("phases")) {
            strings(payload.get("phases"))
        } else {
            None
        },
        reason,
        exempt_phases: if truthy(payload.get("exemptPhases")) {
            strings(payload.get("exemptPhases"))
        } else if parsed_reason.is_none() {
            Some(Vec::new())
        } else {
            None
        },
        dry_run: false,
        settings: if truthy(payload.get("settings")) {
            payload
                .get("settings")
                .and_then(MaintenanceRunSettings::from_value)
        } else {
            None
        },
    };
    let run = match runner
        .run(
            db,
            request,
            MaintenanceContext {
                user_id: Some(job.user_id.clone()),
                time_budget_ms: Some(MAINTENANCE_JOB_TIME_BUDGET_MS),
                now_ms: None,
                mode: MaintenanceMode::Job,
                elapsed: None,
                host_zone_fallback: host_zone_fallback.to_string(),
            },
        )
        .await
    {
        Ok(run) => run,
        Err(e) => return JobOutcome::Failed(crate::db::fallback::error_text(&e)),
    };
    let summary = summarize_maintenance(&run.report);
    let ran: Vec<&str> = summary.ran().iter().map(|p| p.as_str()).collect();
    tracing::info!(
        module = MODULE,
        jobId = %job.id,
        characterId = %character_id,
        reason = summary.reason.as_str(),
        ranJson = %serde_json::to_string(&ran).unwrap_or_default(),
        folded = %null_or(summary.count("fold", "folded")),
        digestsCreated = %null_or(summary.count("consolidation", "digestsCreated")),
        demoted = %null_or(summary.count("demote", "demoted")),
        retired = %null_or(summary.count("retire", "retired")),
        durationMs = %null_or(summary.duration_ms()),
        "Maintenance job complete"
    );
    stash(
        &job.id,
        MaintenanceJobResult {
            summary,
            touched_subjects: run.touched_subjects,
            rows_changed: run.rows_changed,
        },
    );
    JobOutcome::Completed(None)
}

/// The registered `MEMORY_MAINTENANCE` handler over a [`MaintenanceRunner`].
pub struct MemoryMaintenanceJobHandler {
    pub runner: Arc<dyn MaintenanceRunner>,
    /// The host's IANA zone (the memory calendar's last resort).
    pub host_zone_fallback: String,
}

impl JobHandler for MemoryMaintenanceJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(handle_memory_maintenance(
            db,
            self.runner.as_ref(),
            job,
            &self.host_zone_fallback,
        ))
    }
}
