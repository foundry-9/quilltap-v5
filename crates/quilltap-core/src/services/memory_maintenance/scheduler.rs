//! The daily maintenance sweep — v4 `lib/memory/maintenance/scheduler.ts`
//! (`104e986c7`, at the `83334b9a5` pin). The timers are the host's (the
//! cadence lives with the host driver — `quilltap-host`'s maintenance loop);
//! the two bodies they run are here.
//!
//! v4's why, carried: "One scheduler owns the daily tick, the startup grace
//! and the 'a scheduled run completed within 20 hours' startup skip. Each tick
//! enqueues one `MEMORY_MAINTENANCE` job per live character that holds
//! memories; the job resolves which phases its settings allow. The old
//! 30-minute gap between consolidation and housekeeping is simply phase order
//! now." There is no per-character backlog gate (the old
//! `SCHEDULED_MIN_UNCONSIDERED`) and no per-user `autoHousekeepingSettings`
//! gate — every live holder of memories gets a job.

use serde_json::{json, Map, Value};

use crate::db::background_jobs::BackgroundJobsRepository;
use crate::db::runtime::Db;
use crate::db::{characters_read, memories_read, DbError};
use crate::episodic::js_date_parse_ms;
use crate::memory_settings::resolve_memory_settings;
use crate::services::queue_service::{enqueue_memory_maintenance_many, MEMORY_MAINTENANCE};

pub use crate::services::job_scheduler::{
    DAILY_INTERVAL_MS as MAINTENANCE_INTERVAL_MS, RECENT_RUN_WINDOW_MS, STARTUP_GRACE_MS,
};

/// v4's child-logger binding (`module: 'memory:maintenance:scheduler'`).
pub const MODULE: &str = "memory:maintenance:scheduler";

/// v4 `runStartupTick(nowMs)`'s gate: false (INFO `Skipping startup
/// maintenance tick — a scheduled run completed recently {jobId,
/// completedAt}`) when a COMPLETED `MEMORY_MAINTENANCE` job whose payload's
/// `reason` is `scheduled` has `updatedAt ≥ now − 20 h` among the last 50.
/// v4's catch (WARN `Recent-run check failed; running the startup
/// maintenance tick anyway`) is unreachable through v4's real code — the read
/// is a fallback `safeQuery` (`Error finding entities by filter`, `[]`), so a
/// failed read simply finds no recent run. Pinned negative.
pub fn should_run_startup_tick(db: &Db, now_ms: i64) -> bool {
    let recent = db
        .read_main(|c| {
            Ok(crate::db::fallback::find_by_filter_or_empty(
                "background_jobs",
                || BackgroundJobsRepository::new(c).find_recent_by_type(MEMORY_MAINTENANCE, 50),
            ))
        })
        .unwrap_or_default();
    let cutoff = (now_ms - RECENT_RUN_WINDOW_MS) as f64;
    let done = recent.iter().find(|job| {
        if job.status != "COMPLETED" {
            return false;
        }
        let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
        // `Date.parse(updatedAt) >= cutoff` — a NaN compares false.
        payload.get("reason").and_then(Value::as_str) == Some("scheduled")
            && js_date_parse_ms(&job.updated_at).is_some_and(|at| at as f64 >= cutoff)
    });
    if let Some(job) = done {
        tracing::info!(
            module = MODULE,
            jobId = %job.id,
            completedAt = %job.updated_at,
            "Skipping startup maintenance tick — a scheduled run completed recently"
        );
        return false;
    }
    true
}

/// v4's `{charactersScanned, jobsEnqueued}`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScheduledMaintenanceResult {
    pub characters_scanned: usize,
    pub jobs_enqueued: usize,
}

/// v4 `runScheduledMemoryMaintenance()` (`scheduler.ts:101-141`): "one
/// scheduled pass: a job per live character holding memories, when either
/// maintenance or consolidation is on. The enqueue dedupes per character, so
/// the sweep and a watermark run never work on one character at once (bug
/// 221)." Both switches off → DEBUG `Scheduled maintenance skipped:
/// maintenance and consolidation are both off`; every
/// `findDistinctCharacterIds` holder read raw (missing / archived skipped),
/// grouped by owner in first-seen order (WARN `Failed to read a character for
/// scheduled maintenance {characterId, error}`), one `…Many` enqueue per owner
/// (WARN `Failed to enqueue scheduled maintenance for a user {userId,
/// characters, error}`), INFO `Scheduled maintenance pass complete
/// {charactersScanned, jobsEnqueued}`.
pub async fn run_scheduled_memory_maintenance(
    db: &Db,
) -> Result<ScheduledMaintenanceResult, DbError> {
    let mut result = ScheduledMaintenanceResult::default();
    let settings = db.read_main(|c| Ok(resolve_memory_settings(c, None)))?;
    if !settings.maintenance.settings.enabled && !settings.maintenance.consolidation.enabled {
        tracing::debug!(
            module = MODULE,
            "Scheduled maintenance skipped: maintenance and consolidation are both off"
        );
        return Ok(result);
    }
    // "Live characters holding memories, by owner: each owner's in-flight jobs
    // are read once."
    let ids = db.read_main(|c| Ok(memories_read::find_distinct_character_ids(c)))?;
    let mut by_user: Vec<(String, Vec<String>)> = Vec::new();
    for character_id in ids {
        result.characters_scanned += 1;
        let cid = character_id.clone();
        let read = db.read_main(|c| Ok(characters_read::find_by_id_raw_or_none(c, &cid)));
        let character = match read {
            Ok(c) => c,
            Err(error) => {
                tracing::warn!(
                    module = MODULE,
                    characterId = %character_id,
                    error = %crate::db::fallback::error_text(&error),
                    "Failed to read a character for scheduled maintenance"
                );
                continue;
            }
        };
        let Some(character) = character.filter(|c| !crate::api::characters::is_archived(c)) else {
            continue;
        };
        let user_id = character
            .get("userId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        match by_user.iter_mut().find(|(u, _)| *u == user_id) {
            Some((_, list)) => list.push(character_id),
            None => by_user.push((user_id, vec![character_id])),
        }
    }
    for (user_id, character_ids) in &by_user {
        let mut payload = Map::new();
        payload.insert("reason".into(), json!("scheduled"));
        match enqueue_memory_maintenance_many(db, user_id, character_ids, payload, None).await {
            Ok(ids) => result.jobs_enqueued += ids.len(),
            Err(error) => tracing::warn!(
                module = MODULE,
                userId = %user_id,
                characters = character_ids.len(),
                error = %crate::db::fallback::error_text(&error),
                "Failed to enqueue scheduled maintenance for a user"
            ),
        }
    }
    tracing::info!(
        module = MODULE,
        charactersScanned = result.characters_scanned,
        jobsEnqueued = result.jobs_enqueued,
        "Scheduled maintenance pass complete"
    );
    Ok(result)
}
