//! The Data & System server surface (P4.9G1) — the dispatch handlers behind the
//! Settings "Data & System" tab's job-queue + delete-all cards, plus the host
//! job-pump control seam.
//!
//! This module ports v4's tasks-queue / jobs / concurrency handlers
//! (`app/api/v1/system/tools/route.ts` + `system/jobs*`) as free functions over
//! the already-complete [`crate::db::background_jobs`] repo, and the
//! delete-all-data verbs (which compose [`crate::services::delete_all`]).
//!
//! ## Processor status is host-owned (P4.0)
//!
//! v4's `getProcessorStatus()` reports on a forked child process. The host owns
//! ALL cadence in v5, so processor start/stop/status + `wakeProcessor` ride the
//! [`JobPumpControl`] seam on `EngineAssembly`. A `None` seam (read-only
//! embedders, canned assemblies) → the engine arms answer the loud
//! not-assembled refusal. The free functions here take the host-supplied
//! [`ProcessorStatus`] / concurrency value as parameters so the differential can
//! direct-drive them with a pinned status (P4.d7's direct-drive recipe).

use serde::Serialize;
use serde_json::{Map, Value};

use crate::db::background_jobs::{
    BackgroundJob, BackgroundJobsRepository, BjCreate, CreateOptions,
};
use crate::db::runtime::Db;
use crate::db::{characters_read, instance_settings, js_number_to_json, DbError};

use super::types::{ErrorKind, Response};

fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}
fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn not_found(resource: &str) -> Response {
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}

// ── The host job-pump control seam ───────────────────────────────────────────

/// v4 `getProcessorStatus()` (`background-jobs/host/processor-host.ts:321`) — the
/// child-process dispatcher snapshot surfaced by the tasks-queue reads.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessorStatus {
    pub running: bool,
    pub processing: bool,
    pub in_flight: i64,
    pub child_crashed: bool,
}

impl ProcessorStatus {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("running".into(), Value::Bool(self.running));
        m.insert("processing".into(), Value::Bool(self.processing));
        m.insert("inFlight".into(), Value::from(self.in_flight));
        m.insert("childCrashed".into(), Value::Bool(self.child_crashed));
        Value::Object(m)
    }
}

/// The host-owned background-job pump control (P4.9G1). `None` on
/// `EngineAssembly` → the tasks-queue / control / concurrency-set arms answer
/// the loud not-assembled refusal. All four operations are synchronous (v4's
/// processor surface is sync).
pub trait JobPumpControl: Send + Sync {
    /// v4 `getProcessorStatus()`.
    fn status(&self) -> ProcessorStatus;
    /// v4 `startProcessor()` (idempotent `ensureProcessorRunning`).
    fn start(&self);
    /// v4 `stopProcessor()`.
    fn stop(&self);
    /// v4 `wakeProcessor()` — nudge the dispatcher so a new cap applies now.
    fn wake(&self);
}

/// ## ⚠ DELIBERATE DIVERGENCE (dogfood #60, 2026-08-03) — hold the pump still
///
/// A restore truncates and repopulates 43 tables across three partitions, and a
/// `delete_all` empties them. v4 runs neither with its job processor stopped, so
/// a handler can claim a job and write into the middle of it — and the 2026-08-03
/// walk watched scheduled jobs run straight through a restore. Under the
/// standing 2026-08-03 backup/restore ruling that v4 does not do it either is
/// not a defence in this family.
///
/// This guard stops the pump for the duration of an operation and starts it
/// again on **every** exit — the early `return`s, the `?`s, and a panic — which
/// is the whole reason it is a guard and not two calls. Constructing it is the
/// stop; dropping it is the start.
///
/// ### What it does and does not guarantee
///
/// `stop()` clears the shared `running` gate that `pump_loop` checks **before**
/// `pump_claim()`, so no NEW job is claimed while the guard is alive. A job
/// already in flight when the guard is taken runs to completion: v5 never kills
/// a handler mid-job (the documented divergence from v4's SIGTERM), and the
/// runner exposes no in-flight counter to wait on. Narrowing that last window
/// needs an in-flight count on the host runner and is recorded as a named
/// deferral rather than silently assumed away.
///
/// A `None` pump (read-only embedders, canned assemblies) is a no-op: those
/// hosts have no cadence to hold.
pub struct PumpPause {
    pump: Option<std::sync::Arc<dyn JobPumpControl>>,
    /// Whether the pump was running when the guard was taken. A pump the
    /// operator had already stopped by hand (the Tasks Queue Stop button) must
    /// still be stopped afterwards — restarting it would override a deliberate
    /// choice with a side effect.
    was_running: bool,
}

impl PumpPause {
    /// Stop the pump (if there is one) and hold it stopped until the guard drops.
    pub fn new(pump: Option<std::sync::Arc<dyn JobPumpControl>>) -> Self {
        let was_running = match &pump {
            Some(p) => {
                let running = p.status().running;
                p.stop();
                running
            }
            None => false,
        };
        Self { pump, was_running }
    }
}

impl Drop for PumpPause {
    fn drop(&mut self) {
        if !self.was_running {
            return;
        }
        if let Some(p) = &self.pump {
            p.start();
        }
    }
}

// ── Pure helpers (v4 `route.ts` `estimateTokensForJob` / `getJobTypeName`) ────

/// v4 `estimateTokensForJob(job)` (`tools/route.ts:80`). `payload` is the parsed
/// job payload; string lengths use `ceil(len/4)`.
pub fn estimate_tokens_for_job(job_type: &str, payload: &Value) -> i64 {
    let base = 500i64;
    let str_len = |key: &str| -> i64 {
        payload
            .get(key)
            .and_then(Value::as_str)
            .map(|s| s.chars().count() as i64)
            .unwrap_or(0)
    };
    let ceil_div4 = |n: i64| -> i64 { (n + 3) / 4 };
    match job_type {
        "MEMORY_EXTRACTION" => {
            base + ceil_div4(str_len("userMessage")) + ceil_div4(str_len("assistantMessage")) + 300
        }
        "INTER_CHARACTER_MEMORY" => {
            base + ceil_div4(str_len("userMessage")) + ceil_div4(str_len("assistantMessage")) + 400
        }
        "CONTEXT_SUMMARY" => base + 2000,
        "TITLE_UPDATE" => base + 300,
        "CHAT_DANGER_CLASSIFICATION" => base + 1000,
        "SCENE_STATE_TRACKING" => base + 800,
        "LLM_LOG_CLEANUP"
        | "EMBEDDING_GENERATE"
        | "EMBEDDING_REFIT"
        | "EMBEDDING_REINDEX_ALL"
        | "CHARACTER_AVATAR_GENERATION"
        | "CONVERSATION_RENDER"
        | "STORY_BACKGROUND_GENERATION" => 0,
        _ => base,
    }
}

/// v4 `getJobTypeName(type)` (`tools/route.ts:126`) — the display-name lookup;
/// unknown types fall back to the raw enum string.
pub fn job_type_name(job_type: &str) -> &str {
    match job_type {
        "MEMORY_EXTRACTION" => "Memory Extraction",
        "INTER_CHARACTER_MEMORY" => "Character Memory",
        "CONTEXT_SUMMARY" => "Context Summary",
        "TITLE_UPDATE" => "Title Update",
        "LLM_LOG_CLEANUP" => "LLM Log Cleanup",
        "EMBEDDING_GENERATE" => "Embedding Generation",
        "EMBEDDING_REFIT" => "Vocabulary Refit",
        "EMBEDDING_REINDEX_ALL" => "Re-embed All Memories",
        "STORY_BACKGROUND_GENERATION" => "Story Background",
        "CHAT_DANGER_CLASSIFICATION" => "Danger Classification",
        "SCENE_STATE_TRACKING" => "Scene State Tracking",
        "CHARACTER_AVATAR_GENERATION" => "Avatar Generation",
        "CONVERSATION_RENDER" => "Conversation Render",
        other => other,
    }
}

fn parse_payload(job: &BackgroundJob) -> Value {
    serde_json::from_str(&job.payload).unwrap_or_else(|_| Value::Object(Map::new()))
}

/// The full hydrated `BackgroundJob` row (v4 `BackgroundJobSchema` shape): NULL
/// columns are ABSENT (the sqlite backend omits them — the llm-logs-proven
/// hydrate behavior), numbers emitted JS-style (whole floats → integers).
fn job_to_full_json(job: &BackgroundJob) -> Value {
    let mut m = Map::new();
    m.insert("id".into(), Value::from(job.id.clone()));
    m.insert("userId".into(), Value::from(job.user_id.clone()));
    m.insert("type".into(), Value::from(job.job_type.clone()));
    m.insert("status".into(), Value::from(job.status.clone()));
    m.insert("payload".into(), parse_payload(job));
    m.insert("priority".into(), js_number_to_json(job.priority));
    m.insert("attempts".into(), js_number_to_json(job.attempts));
    m.insert("maxAttempts".into(), js_number_to_json(job.max_attempts));
    if let Some(err) = &job.last_error {
        m.insert("lastError".into(), Value::from(err.clone()));
    }
    m.insert("scheduledAt".into(), Value::from(job.scheduled_at.clone()));
    if let Some(v) = &job.started_at {
        m.insert("startedAt".into(), Value::from(v.clone()));
    }
    if let Some(v) = &job.completed_at {
        m.insert("completedAt".into(), Value::from(v.clone()));
    }
    m.insert("createdAt".into(), Value::from(job.created_at.clone()));
    m.insert("updatedAt".into(), Value::from(job.updated_at.clone()));
    Value::Object(m)
}

// ── tasks-queue GET (v4 `handleTasksQueue`) ──────────────────────────────────

/// v4 `GET /api/v1/system/tools?action=tasks-queue` (`route.ts:205`). The
/// processor status + concurrency cap are host-supplied (the seam / instance
/// settings); the DB-derived stats + active job set are built here.
pub fn tasks_queue(
    db: &Db,
    user_id: &str,
    processor_status: &ProcessorStatus,
    max_concurrent_jobs: i64,
) -> Response {
    let user_id = user_id.to_string();
    let built: Result<Value, DbError> = db.read_main(|main| {
        let repo = BackgroundJobsRepository::new(main);
        let stats = repo.get_stats(Some(&user_id))?;
        let pending = repo.find_by_user_id(&user_id, Some("PENDING"))?;
        let processing = repo.find_by_user_id(&user_id, Some("PROCESSING"))?;
        let failed = repo.find_by_user_id(&user_id, Some("FAILED"))?;
        let paused = repo.find_by_user_id(&user_id, Some("PAUSED"))?;

        // Active set: [processing, pending, retry-eligible failed, paused],
        // deduped by id in insertion order (v4 `route.ts:218`).
        let mut seen = std::collections::HashSet::new();
        let mut active: Vec<BackgroundJob> = Vec::new();
        let mut push = |job: BackgroundJob, active: &mut Vec<BackgroundJob>| {
            if seen.insert(job.id.clone()) {
                active.push(job);
            }
        };
        for j in processing {
            push(j, &mut active);
        }
        for j in pending {
            push(j, &mut active);
        }
        for j in failed {
            if j.attempts < j.max_attempts {
                push(j, &mut active);
            }
        }
        for j in paused {
            push(j, &mut active);
        }

        // Sort: priority DESC, then scheduledAt ASC (v4 `route.ts:231`).
        active.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    let am = crate::clock::iso_to_ms(&a.scheduled_at).unwrap_or(0);
                    let bm = crate::clock::iso_to_ms(&b.scheduled_at).unwrap_or(0);
                    am.cmp(&bm)
                })
        });

        // Character-name cache (payload.characterId → character.name) for jobs
        // whose payload sets characterId but not characterName.
        let mut name_cache: std::collections::HashMap<String, Option<String>> =
            std::collections::HashMap::new();
        let mut total_tokens = 0i64;
        let mut jobs_out: Vec<Value> = Vec::with_capacity(active.len());
        for job in &active {
            let payload = parse_payload(job);
            let tokens = estimate_tokens_for_job(&job.job_type, &payload);
            total_tokens += tokens;

            let mut m = Map::new();
            m.insert("id".into(), Value::from(job.id.clone()));
            m.insert("type".into(), Value::from(job.job_type.clone()));
            m.insert(
                "typeName".into(),
                Value::from(job_type_name(&job.job_type).to_string()),
            );
            m.insert("status".into(), Value::from(job.status.clone()));
            m.insert("priority".into(), js_number_to_json(job.priority));
            m.insert("attempts".into(), js_number_to_json(job.attempts));
            m.insert("maxAttempts".into(), js_number_to_json(job.max_attempts));
            m.insert("scheduledAt".into(), Value::from(job.scheduled_at.clone()));
            if let Some(v) = &job.started_at {
                m.insert("startedAt".into(), Value::from(v.clone()));
            }
            if let Some(v) = &job.last_error {
                m.insert("lastError".into(), Value::from(v.clone()));
            }
            m.insert("estimatedTokens".into(), Value::from(tokens));
            if let Some(chat_id) = payload.get("chatId").and_then(Value::as_str) {
                m.insert("chatId".into(), Value::from(chat_id.to_string()));
            }
            // characterName: explicit payload value, else resolve via characterId.
            let char_name = match payload.get("characterName").and_then(Value::as_str) {
                Some(n) => Some(n.to_string()),
                None => match payload.get("characterId").and_then(Value::as_str) {
                    Some(cid) => {
                        if let Some(cached) = name_cache.get(cid) {
                            cached.clone()
                        } else {
                            let resolved =
                                characters_read::find_by_id_raw(main, cid)?.and_then(|c| {
                                    c.get("name").and_then(Value::as_str).map(str::to_string)
                                });
                            name_cache.insert(cid.to_string(), resolved.clone());
                            resolved
                        }
                    }
                    None => None,
                },
            };
            if let Some(name) = char_name {
                m.insert("characterName".into(), Value::from(name));
            }
            jobs_out.push(Value::Object(m));
        }

        // stats bag — key order pending, processing, failed, completed, dead,
        // paused, activeTotal (v4 `route.ts:287`, deliberately not QueueStats order).
        let mut stats_m = Map::new();
        stats_m.insert("pending".into(), Value::from(stats.pending));
        stats_m.insert("processing".into(), Value::from(stats.processing));
        stats_m.insert("failed".into(), Value::from(stats.failed));
        stats_m.insert("completed".into(), Value::from(stats.completed));
        stats_m.insert("dead".into(), Value::from(stats.dead));
        stats_m.insert("paused".into(), Value::from(stats.paused));
        stats_m.insert("activeTotal".into(), Value::from(active.len() as i64));

        let mut root = Map::new();
        root.insert("stats".into(), Value::Object(stats_m));
        root.insert("jobs".into(), Value::Array(jobs_out));
        root.insert("totalEstimatedTokens".into(), Value::from(total_tokens));
        root.insert("processorStatus".into(), processor_status.to_json());
        root.insert("maxConcurrentJobs".into(), Value::from(max_concurrent_jobs));
        Ok(Value::Object(root))
    });

    match built {
        Ok(v) => Response::System(v),
        Err(e) => internal(e),
    }
}

// ── tasks-queue control (v4 `handleTasksQueueControl`) ───────────────────────

/// Validate the `start`|`stop` action; on success returns the post-action
/// response body (the caller runs the seam side-effect between).
pub fn tasks_queue_control_response(action: &str, processor_status: &ProcessorStatus) -> Response {
    let mut m = Map::new();
    m.insert("success".into(), Value::Bool(true));
    m.insert("action".into(), Value::from(action.to_string()));
    m.insert("processorStatus".into(), processor_status.to_json());
    Response::System(Value::Object(m))
}

/// v4's `!['start','stop'].includes(action)` → `badRequest`.
pub fn validate_control_action(action: &str) -> Result<(), Response> {
    if action == "start" || action == "stop" {
        Ok(())
    } else {
        Err(bad_request("Invalid action. Must be \"start\" or \"stop\""))
    }
}

// ── concurrency get/set (v4 `handleJobConcurrency*`) ─────────────────────────

pub fn job_concurrency_get_response(concurrency: i64) -> Response {
    let mut m = Map::new();
    m.insert("success".into(), Value::Bool(true));
    m.insert("concurrency".into(), Value::from(concurrency));
    Response::System(Value::Object(m))
}

/// v4 `jobConcurrencySchema` (`z.number().int().min(1).max(32)`). Out-of-range /
/// non-integer → v5's error envelope (a documented divergence from v4's Zod
/// `details` array — the order maps validation to "v5's error envelope"; the
/// SPA slider is 1..32 so production never trips it).
pub fn validate_concurrency(value: i64) -> Result<i64, Response> {
    if (1..=32).contains(&value) {
        Ok(value)
    } else {
        Err(Response::error(
            ErrorKind::BadRequest,
            "Validation error: maxConcurrentJobs must be an integer between 1 and 32",
        ))
    }
}

pub async fn job_concurrency_set(db: &Db, value: i64) -> Response {
    let concurrency = match validate_concurrency(value) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let written = db
        .write(move |w| {
            instance_settings::set_max_concurrent_jobs(w.main().connection(), concurrency)
        })
        .await;
    match written {
        Ok(()) => job_concurrency_get_response(concurrency),
        Err(e) => internal(e),
    }
}

/// Read the persisted concurrency cap (for the GET verb + the tasks-queue read).
pub fn read_max_concurrent_jobs(db: &Db) -> Result<i64, Response> {
    db.read_main(instance_settings::get_max_concurrent_jobs)
        .map_err(internal)
}

// ── single-job GET / control / delete (v4 `system/jobs/[id]`) ────────────────

pub fn job_get(db: &Db, id: &str) -> Response {
    let id = id.to_string();
    let found = db.read_main(move |main| BackgroundJobsRepository::new(main).find_by_id(&id));
    match found {
        Ok(Some(job)) => {
            let mut m = Map::new();
            m.insert("job".into(), job_to_full_json(&job));
            Response::System(Value::Object(m))
        }
        Ok(None) => not_found("Job"),
        Err(e) => internal(e),
    }
}

/// The pause/resume/delete guard outcome so the engine arm can run the seam
/// side-effect (v4 resumes call `ensureProcessorRunning`).
pub enum JobControlOutcome {
    Responded(Response),
    /// A resume succeeded — the engine arm should nudge the pump before replying.
    Resumed(Response),
}

/// v4 `POST /api/v1/system/jobs/[id]?action=pause|resume`.
pub async fn job_control(db: &Db, id: &str, action: &str) -> JobControlOutcome {
    if action != "pause" && action != "resume" {
        return JobControlOutcome::Responded(bad_request(
            "Invalid action. Available actions: pause, resume",
        ));
    }
    let id_owned = id.to_string();
    let action_owned = action.to_string();
    let result: Result<Response, DbError> = db.write(move |w| {
        let repo = w.main().background_jobs();
        let Some(job) = repo.find_by_id(&id_owned)? else {
            return Ok(not_found("Job"));
        };
        if action_owned == "pause" {
            if job.status != "PENDING" && job.status != "FAILED" {
                return Ok(bad_request(format!(
                    "Cannot pause a job with status \"{}\". Only PENDING or FAILED jobs can be paused.",
                    job.status
                )));
            }
            match repo.pause(&id_owned)? {
                Some(updated) => {
                    let mut m = Map::new();
                    m.insert("job".into(), job_to_full_json(&updated));
                    Ok(Response::System(Value::Object(m)))
                }
                None => Ok(internal("Failed to pause job")),
            }
        } else {
            if job.status != "PAUSED" {
                return Ok(bad_request(format!(
                    "Cannot resume a job with status \"{}\". Only PAUSED jobs can be resumed.",
                    job.status
                )));
            }
            match repo.resume(&id_owned)? {
                Some(updated) => {
                    let mut m = Map::new();
                    m.insert("job".into(), job_to_full_json(&updated));
                    Ok(Response::System(Value::Object(m)))
                }
                None => Ok(internal("Failed to resume job")),
            }
        }
    }).await;
    match result {
        Ok(resp) => {
            let succeeded_resume = action == "resume" && matches!(resp, Response::System(_));
            if succeeded_resume {
                JobControlOutcome::Resumed(resp)
            } else {
                JobControlOutcome::Responded(resp)
            }
        }
        Err(e) => JobControlOutcome::Responded(internal(e)),
    }
}

/// v4 `DELETE /api/v1/system/jobs/[id]` — blocks a PROCESSING job.
pub async fn job_delete(db: &Db, id: &str) -> Response {
    let id_owned = id.to_string();
    let result: Result<Response, DbError> = db
        .write(move |w| {
            let repo = w.main().background_jobs();
            let Some(job) = repo.find_by_id(&id_owned)? else {
                return Ok(not_found("Job"));
            };
            if job.status == "PROCESSING" {
                return Ok(bad_request(
                    "Cannot delete a job that is currently processing",
                ));
            }
            if repo.delete(&id_owned)? {
                let mut m = Map::new();
                m.insert("success".into(), Value::Bool(true));
                Ok(Response::System(Value::Object(m)))
            } else {
                Ok(internal("Failed to delete job"))
            }
        })
        .await;
    match result {
        Ok(resp) => resp,
        Err(e) => internal(e),
    }
}

// ── jobs collection GET / POST (v4 `system/jobs/route.ts`) ───────────────────

/// v4 `GET /api/v1/system/jobs` — stats + activeByType + processor, plus the
/// optional `jobs` (50 newest) and `pendingForChat` legs.
pub fn jobs_list(
    db: &Db,
    user_id: &str,
    include_jobs: bool,
    chat_id: Option<&str>,
    processor_status: &ProcessorStatus,
) -> Response {
    let user_id = user_id.to_string();
    let chat_id = chat_id.map(str::to_string);
    let built: Result<Value, DbError> = db.read_main(|main| {
        let repo = BackgroundJobsRepository::new(main);
        let stats = repo.get_stats(Some(&user_id))?;
        let active_by_type = repo.get_active_counts_by_type(Some(&user_id))?;

        let mut stats_m = Map::new();
        stats_m.insert("pending".into(), Value::from(stats.pending));
        stats_m.insert("processing".into(), Value::from(stats.processing));
        stats_m.insert("completed".into(), Value::from(stats.completed));
        stats_m.insert("failed".into(), Value::from(stats.failed));
        stats_m.insert("dead".into(), Value::from(stats.dead));
        stats_m.insert("paused".into(), Value::from(stats.paused));

        let mut by_type_m = Map::new();
        for (t, c) in active_by_type {
            by_type_m.insert(t, Value::from(c));
        }

        let mut root = Map::new();
        root.insert("stats".into(), Value::Object(stats_m));
        root.insert("activeByType".into(), Value::Object(by_type_m));
        root.insert("processor".into(), processor_status.to_json());
        if include_jobs {
            let jobs: Vec<Value> = repo
                .find_by_user_id(&user_id, None)?
                .into_iter()
                .take(50)
                .map(|j| job_to_full_json(&j))
                .collect();
            root.insert("jobs".into(), Value::Array(jobs));
        }
        if let Some(cid) = &chat_id {
            let pending: Vec<Value> = repo
                .find_pending_for_chat(cid)?
                .into_iter()
                .map(|j| job_to_full_json(&j))
                .collect();
            root.insert("pendingForChat".into(), Value::Array(pending));
        }
        Ok(Value::Object(root))
    });
    match built {
        Ok(v) => Response::System(v),
        Err(e) => internal(e),
    }
}

/// v4's `BackgroundJobTypeEnum` (23 values) — the enqueue type gate.
const JOB_TYPES: &[&str] = &[
    "MEMORY_EXTRACTION",
    "INTER_CHARACTER_MEMORY",
    "CONTEXT_SUMMARY",
    "TITLE_UPDATE",
    "LLM_LOG_CLEANUP",
    "EMBEDDING_GENERATE",
    "EMBEDDING_REFIT",
    "EMBEDDING_REINDEX_ALL",
    "EMBEDDING_REAPPLY_PROFILE",
    "STORY_BACKGROUND_GENERATION",
    "CHAT_DANGER_CLASSIFICATION",
    "SCENE_STATE_TRACKING",
    "CHARACTER_AVATAR_GENERATION",
    "CONVERSATION_RENDER",
    "MEMORY_HOUSEKEEPING",
    "MEMORY_REGENERATE_CHAT",
    "MEMORY_REGENERATE_ALL",
    "WARDROBE_OUTFIT_ANNOUNCEMENT",
    "AUTONOMOUS_ROOM_TURN",
    "AUTONOMOUS_ROOM_SCHEDULE_TICK",
    "CARINA_MEMORY_EXTRACTION",
    "CHARACTER_HEADSHOULDERS_BACKFILL",
    "REGENERATE_CONVERSATION_SUMMARIES",
];

/// v4 `POST /api/v1/system/jobs` — enqueue a job (201). Returns the id + the
/// fixed message. The engine arm nudges the pump after a success.
#[allow(clippy::too_many_arguments)]
pub async fn jobs_enqueue(
    db: &Db,
    user_id: &str,
    job_type: &str,
    payload: &Value,
    priority: Option<f64>,
    max_attempts: Option<f64>,
    new_id: &str,
    now_iso: &str,
) -> Response {
    if !JOB_TYPES.contains(&job_type) {
        return bad_request(format!(
            "Invalid job type. Must be one of: {}",
            JOB_TYPES.join(", ")
        ));
    }
    if !payload.is_object() {
        return bad_request("Payload is required and must be an object");
    }
    let create = BjCreate {
        user_id: user_id.to_string(),
        job_type: job_type.to_string(),
        status: Some("PENDING".to_string()),
        payload: payload.clone(),
        priority: priority.unwrap_or(0.0),
        attempts: 0.0,
        max_attempts: max_attempts.unwrap_or(3.0),
        last_error: None,
        scheduled_at: now_iso.to_string(),
        started_at: None,
        completed_at: None,
    };
    let opts = CreateOptions {
        id: new_id.to_string(),
        created_at: now_iso.to_string(),
        updated_at: now_iso.to_string(),
    };
    let written = db
        .write(move |w| w.main().background_jobs().create(&create, &opts))
        .await;
    match written {
        Ok(()) => {
            let mut m = Map::new();
            m.insert("jobId".into(), Value::from(new_id.to_string()));
            m.insert("message".into(), Value::from("Job created successfully"));
            Response::System(Value::Object(m))
        }
        Err(e) => internal(e),
    }
}

/// [`jobs_enqueue`] with the id + timestamp minted here (v4's repo `create`
/// mints both). The collection POST is a web-edge-only leg — the §1 wire surface
/// has no verb for it — so the minting must not leak to the transport;
/// [`jobs_enqueue`] keeps taking them explicitly so the differential can pin
/// them.
pub async fn jobs_enqueue_now(
    db: &Db,
    user_id: &str,
    job_type: &str,
    payload: &Value,
    priority: Option<f64>,
    max_attempts: Option<f64>,
) -> Response {
    jobs_enqueue(
        db,
        user_id,
        job_type,
        payload,
        priority,
        max_attempts,
        &uuid::Uuid::new_v4().to_string(),
        &crate::clock::now_iso(),
    )
    .await
}

// ── delete-all-data (v4 `handleDeleteDataPreview` / `handleDeleteData`) ──────

/// v4's server-side confirmation sentinel (`tools/route.ts:155`) — re-checked
/// here independently of whatever the client typed into its dialog.
pub const DELETE_ALL_CONFIRM: &str = "DELETE_ALL_MY_DATA";

fn delete_summary_body(summary: &crate::services::delete_all::DeleteSummary) -> Response {
    let mut m = Map::new();
    m.insert("success".into(), Value::Bool(true));
    m.insert(
        "summary".into(),
        serde_json::to_value(summary).unwrap_or(Value::Null),
    );
    Response::System(Value::Object(m))
}

/// v4 `GET /api/v1/system/tools?action=delete-data-preview` (`route.ts:182`) —
/// counts only, no writes. Body `{success:true, summary}`; a thrown error is
/// v4's `serverError('Failed to preview data deletion')`.
pub fn delete_data_preview(db: &Db, user_id: &str) -> Response {
    match crate::services::delete_all::preview_delete_all_user_data(db, user_id) {
        Ok(summary) => delete_summary_body(&summary),
        Err(e) => {
            tracing::error!(target: "quilltap::system_data", error = %e, "Preview delete failed");
            Response::error(ErrorKind::Internal, "Failed to preview data deletion")
        }
    }
}

/// v4 `POST /api/v1/system/tools?action=delete-data` (`route.ts:149`). The
/// `{confirm:'DELETE_ALL_MY_DATA'}` sentinel is re-checked server-side; a
/// mismatch is v4's `badRequest` with the verbatim message. Body
/// `{success:true, summary}`; a thrown error is `serverError('Failed to delete
/// data')`.
pub async fn delete_data(
    db: &Db,
    user_id: &str,
    confirm: &str,
    keep_archived_character_bundles: Option<bool>,
) -> Response {
    if confirm != DELETE_ALL_CONFIRM {
        return bad_request(
            "Confirmation required. Send { \"confirm\": \"DELETE_ALL_MY_DATA\" }".to_string(),
        );
    }
    let options = crate::services::delete_all::DeleteUserDataOptions {
        keep_archived_character_bundles,
    };
    match crate::services::delete_all::delete_all_user_data(db, user_id, options).await {
        Ok(summary) => delete_summary_body(&summary),
        Err(e) => {
            tracing::error!(target: "quilltap::system_data", error = %e, "Delete all data failed");
            Response::error(ErrorKind::Internal, "Failed to delete data")
        }
    }
}

#[cfg(test)]
mod pump_pause_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A pump that records what it was told, in order, and reports the same
    /// `running` gate the host's real one does.
    #[derive(Default)]
    struct FakePump {
        running: AtomicBool,
        starts: AtomicUsize,
        stops: AtomicUsize,
    }

    impl JobPumpControl for FakePump {
        fn status(&self) -> ProcessorStatus {
            ProcessorStatus {
                running: self.running.load(Ordering::SeqCst),
                processing: false,
                in_flight: 0,
                child_crashed: false,
            }
        }
        fn start(&self) {
            self.running.store(true, Ordering::SeqCst);
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
        fn stop(&self) {
            self.running.store(false, Ordering::SeqCst);
            self.stops.fetch_add(1, Ordering::SeqCst);
        }
        fn wake(&self) {}
    }

    fn running_pump() -> Arc<FakePump> {
        let p = Arc::new(FakePump::default());
        p.running.store(true, Ordering::SeqCst);
        p
    }

    #[test]
    fn the_pump_is_stopped_for_the_whole_body_and_started_after() {
        let pump = running_pump();
        {
            let _guard = PumpPause::new(Some(pump.clone() as Arc<dyn JobPumpControl>));
            // This is the claim window: anything the body does happens here, and
            // `pump_loop` checks exactly this gate before claiming.
            assert!(
                !pump.status().running,
                "no job may be claimed while the guard is alive"
            );
        }
        assert!(pump.status().running, "the pump must come back");
        assert_eq!(pump.stops.load(Ordering::SeqCst), 1);
        assert_eq!(pump.starts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_early_return_still_restarts_it() {
        let pump = running_pump();
        fn body(pump: Arc<FakePump>) -> Result<(), &'static str> {
            let _guard = PumpPause::new(Some(pump.clone() as Arc<dyn JobPumpControl>));
            assert!(!pump.status().running);
            Err("the restore failed") // the `?`-shaped exit the guard exists for
        }
        assert!(body(pump.clone()).is_err());
        assert!(
            pump.status().running,
            "a failed restore must not leave the pump stopped forever"
        );
    }

    #[test]
    fn a_panic_still_restarts_it() {
        let pump = running_pump();
        let p = pump.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = PumpPause::new(Some(p.clone() as Arc<dyn JobPumpControl>));
            panic!("mid-restore");
        }));
        assert!(result.is_err());
        assert!(pump.status().running, "unwinding must still restart it");
    }

    #[test]
    fn a_pump_the_operator_had_already_stopped_stays_stopped() {
        // The Tasks Queue Stop button is a deliberate choice; a restore must not
        // quietly undo it on the way out.
        let pump = Arc::new(FakePump::default()); // `running` false from the start
        {
            let _guard = PumpPause::new(Some(pump.clone() as Arc<dyn JobPumpControl>));
        }
        assert!(!pump.status().running);
        assert_eq!(
            pump.starts.load(Ordering::SeqCst),
            0,
            "the guard must not start a pump that was not running"
        );
    }

    #[test]
    fn no_pump_is_a_no_op() {
        // Read-only embedders and canned assemblies have no cadence to hold.
        drop(PumpPause::new(None));
    }
}
