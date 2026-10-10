//! Consolidation triggers — v4 `lib/memory/consolidation-triggers.ts`
//! (`d58548051`, byte-identical at the `01a83539d` pin).
//!
//! Two ways a `MEMORY_CONSOLIDATION` job is enqueued automatically (both
//! gated on `memoryConsolidation.enabled`, OFF by default):
//!
//!   - **Scheduled** — the daily housekeeping pass runs
//!     [`run_scheduled_consolidation`] FIRST: one job per live character
//!     holding at least [`SCHEDULED_MIN_UNCONSIDERED`] never-considered hot
//!     rows.
//!   - **Watermark** — after a memory-writing job completes, every character it
//!     wrote rows for is checked: past `watermark` never-considered hot rows, a
//!     run is enqueued, unless one ran inside the throttle window. The
//!     watermark path has to stay cheap — it runs after every such job — so it
//!     reads one setting and returns at once when consolidation is off, and
//!     debounces the COUNT per character.
//!
//! **The watermark's v5 shape (P4.D271 R-B, a recorded divergence).** v4 runs
//! the check in the dispatcher's post-commit hook over the child's buffered
//! writes (`characterIdsFromMemoryWrites` over `memories.create`). v5 has no
//! child and no write list, so [`check_consolidation_watermark_after_job`] —
//! ONE call in the job runner's completion region — collects the characters
//! whose `memories` rows were INSERTED past a SQLite `rowid` high-water mark
//! taken when the job was claimed. NOT a `createdAt` window: every extraction
//! path backdates `createdAt` to the source message, so such a window is empty
//! for every real extraction. The claim-time mark is realized from a
//! process-global, time-stamped mark log: every region call (any job type, only
//! while enabled) records `(wall-clock ms, MAX(rowid))`; at a job's completion
//! the claim-time mark is the NEWEST mark stamped at or before that job's
//! `startedAt`. A mark taken at or before claim can only OVER-include
//! (concurrent jobs' and inline writes' rows), which v4's own 10-minute debounce
//! and 6-hour throttle absorb; a job with no mark at or before its claim (the
//! first after boot, or after a disabled→enabled flip) is not inspected. Marks
//! older than [`MARK_RETENTION_MS`] are pruned, the newest always kept. Pinned
//! both ways as `WATERMARK_ROWID_WINDOW` in
//! `consolidation_triggers_tier2_equivalence`. The call is AWAITED inline (v4's
//! `void` is fire-and-forget; awaiting keeps the pump deterministic).

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

use crate::db::background_jobs::{BackgroundJob, BackgroundJobsRepository};
use crate::db::instance_settings::get_memory_consolidation_settings;
use crate::db::runtime::Db;
use crate::db::{characters_read, memories_read, DbError};
use crate::services::queue_service::{enqueue_memory_consolidation, MemoryConsolidationPayload};

const MODULE: &str = "memory:consolidation-triggers";

/// Minimum gap between watermark COUNTs for one character.
pub const WATERMARK_CHECK_DEBOUNCE_MS: i64 = 10 * 60 * 1000;

/// A watermark run is not re-enqueued while the character's last run is
/// younger than this.
pub const WATERMARK_RUN_THROTTLE_MS: i64 = 6 * 60 * 60 * 1000;

/// Characters need at least this many never-considered hot rows for a
/// scheduled run to be worth a job.
pub const SCHEDULED_MIN_UNCONSIDERED: i64 = 2;

/// R-B: marks older than this are pruned (past `STUCK_JOB_TIMEOUT_MINUTES` = 10
/// plus v4's 7-minute job budget).
pub const MARK_RETENTION_MS: i64 = 15 * 60 * 1000;

fn last_watermark_check() -> &'static Mutex<HashMap<String, i64>> {
    static MAP: std::sync::OnceLock<Mutex<HashMap<String, i64>>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// R-B's mark log: `(wall-clock ms, MAX(rowid))`, oldest first.
fn mark_log() -> &'static Mutex<Vec<(i64, i64)>> {
    static LOG: std::sync::OnceLock<Mutex<Vec<(i64, i64)>>> = std::sync::OnceLock::new();
    LOG.get_or_init(|| Mutex::new(Vec::new()))
}

/// Test hook — v4 `resetConsolidationTriggerStateForTests` (plus R-B's mark
/// log).
pub fn reset_consolidation_trigger_state_for_tests() {
    last_watermark_check().lock().expect("debounce map").clear();
    mark_log().lock().expect("mark log").clear();
}

/// Test hook: plant a mark (R-B) as a prior region call would have.
pub fn record_mark_for_tests(at_ms: i64, max_rowid: i64) {
    mark_log()
        .lock()
        .expect("mark log")
        .push((at_ms, max_rowid));
}

/// v4 `ranRecently`: a `MEMORY_CONSOLIDATION` job for this character ran (or is
/// running) inside the throttle window — not a dry run; COMPLETED, PROCESSING
/// or PENDING; `updatedAt` younger than the throttle.
fn ran_recently(db: &Db, character_id: &str, now_ms: i64) -> bool {
    let recent = db
        .read_main(|c| {
            BackgroundJobsRepository::new(c).find_recent_by_type("MEMORY_CONSOLIDATION", 50)
        })
        .unwrap_or_default();
    recent.iter().any(|job| {
        let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
        if payload.get("characterId").and_then(Value::as_str) != Some(character_id)
            || payload.get("dryRun") == Some(&Value::Bool(true))
        {
            return false;
        }
        if !matches!(job.status.as_str(), "COMPLETED" | "PROCESSING" | "PENDING") {
            return false;
        }
        let ts = if job.updated_at.is_empty() {
            0
        } else {
            crate::episodic::js_date_parse_ms(&job.updated_at).unwrap_or(i64::MIN)
        };
        // JS: `nowMs - NaN < throttle` is false.
        ts != i64::MIN && now_ms - ts < WATERMARK_RUN_THROTTLE_MS
    })
}

/// v4 `maybeEnqueueConsolidationForCharacters`: the watermark check for a set
/// of characters. Never fails — a failure here must not disturb the job whose
/// completion called it. Returns the characters a run was enqueued for.
pub async fn maybe_enqueue_consolidation_for_characters(
    db: &Db,
    character_ids: &[String],
    now_ms: i64,
) -> Vec<String> {
    let mut enqueued: Vec<String> = Vec::new();
    if character_ids.is_empty() {
        return enqueued;
    }
    let result: Result<(), DbError> = async {
        let settings = db.read_main(|c| Ok(get_memory_consolidation_settings(c)))?;
        if !settings.enabled {
            return Ok(());
        }
        for character_id in character_ids {
            {
                let mut map = last_watermark_check().lock().expect("debounce map");
                if let Some(last) = map.get(character_id) {
                    if now_ms - last < WATERMARK_CHECK_DEBOUNCE_MS {
                        continue;
                    }
                }
                map.insert(character_id.clone(), now_ms);
            }

            let unconsidered =
                db.read_main(|c| memories_read::count_unconsidered_hot(c, character_id))?;
            if unconsidered <= settings.watermark {
                tracing::debug!(
                    module = MODULE,
                    characterId = %character_id,
                    unconsidered,
                    watermark = settings.watermark,
                    "Consolidation watermark not reached"
                );
                continue;
            }
            if ran_recently(db, character_id, now_ms) {
                tracing::debug!(
                    module = MODULE,
                    characterId = %character_id,
                    unconsidered,
                    throttleMs = WATERMARK_RUN_THROTTLE_MS,
                    "Consolidation watermark reached but a recent run is inside the throttle window"
                );
                continue;
            }
            let character =
                db.read_main(|c| Ok(characters_read::find_by_id_raw_or_none(c, character_id)))?;
            let Some(character) = character else { continue };
            if crate::api::characters::is_archived(&character) {
                continue;
            }
            let user_id = character
                .get("userId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let job_id = enqueue_memory_consolidation(
                db,
                user_id,
                &MemoryConsolidationPayload {
                    character_id: character_id.clone(),
                    dry_run: None,
                    max_clusters_per_run: None,
                    trigger: Some("watermark".to_string()),
                },
            )
            .await?;
            enqueued.push(character_id.clone());
            tracing::info!(
                module = MODULE,
                characterId = %character_id,
                unconsidered,
                watermark = settings.watermark,
                jobId = %job_id,
                "Consolidation watermark reached; run enqueued"
            );
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(
            module = MODULE,
            characterIdsJson = %serde_json::to_string(character_ids).unwrap_or_default(),
            error = %crate::db::fallback::error_text(&e),
            "Consolidation watermark check failed (non-fatal)"
        );
    }
    enqueued
}

/// R-B: the claim-time rowid mark for a job claimed at `started_ms` — the
/// NEWEST mark stamped at or before it.
fn claim_mark(marks: &[(i64, i64)], started_ms: i64) -> Option<i64> {
    marks
        .iter()
        .filter(|(at, _)| *at <= started_ms)
        .max_by_key(|(at, _)| *at)
        .map(|(_, rowid)| *rowid)
}

/// R-B: prune marks older than [`MARK_RETENTION_MS`], keeping the newest.
fn prune_marks(marks: &mut Vec<(i64, i64)>, now_ms: i64) {
    let newest = marks.iter().map(|(at, _)| *at).max();
    marks.retain(|(at, _)| now_ms - at <= MARK_RETENTION_MS || Some(*at) == newest);
}

/// The characters whose `memories` rows were inserted past `rowid` (R-B's
/// window).
pub fn characters_inserted_since(db: &Db, rowid: i64) -> Result<Vec<String>, DbError> {
    db.read_main(|c| {
        let mut stmt =
            c.prepare("SELECT DISTINCT characterId FROM memories WHERE rowid > ?1 AND characterId IS NOT NULL")?;
        let rows = stmt.query_map([rowid], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
}

/// `SELECT MAX(rowid) FROM memories` (0 for an empty table).
pub fn max_memory_rowid(db: &Db) -> Result<i64, DbError> {
    db.read_main(|c| {
        Ok(
            c.query_row("SELECT COALESCE(MAX(rowid), 0) FROM memories", [], |r| {
                r.get::<_, i64>(0)
            })?,
        )
    })
}

/// v4 `checkConsolidationWatermarkAfterCommit` → `maybeEnqueueConsolidationAfterCommit`,
/// in R-B's v5 shape (see the module docs). The consolidation job's own digests
/// never re-trigger it; a dry-run job wrote nothing. v4's catch around it
/// (`Consolidation watermark check skipped after job`) is UNREACHABLE through
/// v4's real code — the callee never throws — so it has no port; v5's own
/// read failures here end the check silently, as that catch would.
pub async fn check_consolidation_watermark_after_job(db: &Db, job: &BackgroundJob) {
    check_consolidation_watermark_after_job_at(db, job, crate::clock::now_unix_ms()).await;
}

/// [`check_consolidation_watermark_after_job`] with an injected wall clock.
pub async fn check_consolidation_watermark_after_job_at(db: &Db, job: &BackgroundJob, now_ms: i64) {
    if job.job_type == "MEMORY_CONSOLIDATION" {
        return;
    }
    let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
    if payload.get("dryRun") == Some(&Value::Bool(true)) {
        return;
    }
    let Ok(settings) = db.read_main(|c| Ok(get_memory_consolidation_settings(c))) else {
        return;
    };
    if !settings.enabled {
        return;
    }
    let Ok(current) = max_memory_rowid(db) else {
        return;
    };
    let window = {
        let mut marks = mark_log().lock().expect("mark log");
        let started_ms = job.started_at.as_deref().and_then(crate::clock::iso_to_ms);
        let found = started_ms.and_then(|s| claim_mark(&marks, s));
        marks.push((now_ms, current));
        prune_marks(&mut marks, now_ms);
        found
    };
    let Some(since) = window else { return };
    let Ok(ids) = characters_inserted_since(db, since) else {
        return;
    };
    if ids.is_empty() {
        return;
    }
    maybe_enqueue_consolidation_for_characters(db, &ids, now_ms).await;
}

/// v4 `runScheduledConsolidation`'s result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScheduledConsolidationResult {
    pub characters_enqueued: usize,
    pub characters_scanned: usize,
}

/// v4 `runScheduledConsolidation`: the daily pass — enqueue a scheduled
/// consolidation run for every live character that holds memories and has at
/// least a couple never considered. A dedupe hit counts as enqueued (v4's
/// enqueue returns the in-flight id). v4's caller-side catch (`Scheduled
/// consolidation pass failed; housekeeping continues`) is reachable in v5 only
/// through a failed settings read (v4: unreachable — every read here is a
/// fallback); recorded.
pub async fn run_scheduled_consolidation(db: &Db) -> Result<ScheduledConsolidationResult, DbError> {
    let mut result = ScheduledConsolidationResult::default();
    let settings = db.read_main(|c| Ok(get_memory_consolidation_settings(c)))?;
    if !settings.enabled {
        tracing::debug!(module = MODULE, "Scheduled consolidation skipped: disabled");
        return Ok(result);
    }
    let character_ids = db.read_main(|c| Ok(memories_read::find_distinct_character_ids(c)))?;
    for character_id in &character_ids {
        result.characters_scanned += 1;
        let attempt: Result<bool, DbError> = async {
            let character =
                db.read_main(|c| Ok(characters_read::find_by_id_raw_or_none(c, character_id)))?;
            let Some(character) = character else {
                return Ok(false);
            };
            if crate::api::characters::is_archived(&character) {
                return Ok(false);
            }
            let unconsidered =
                db.read_main(|c| memories_read::count_unconsidered_hot(c, character_id))?;
            if unconsidered < SCHEDULED_MIN_UNCONSIDERED {
                return Ok(false);
            }
            let user_id = character
                .get("userId")
                .and_then(Value::as_str)
                .unwrap_or_default();
            enqueue_memory_consolidation(
                db,
                user_id,
                &MemoryConsolidationPayload {
                    character_id: character_id.clone(),
                    dry_run: None,
                    max_clusters_per_run: None,
                    trigger: Some("scheduled".to_string()),
                },
            )
            .await?;
            Ok(true)
        }
        .await;
        match attempt {
            Ok(true) => result.characters_enqueued += 1,
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(
                    module = MODULE,
                    characterId = %character_id,
                    error = %crate::db::fallback::error_text(&e),
                    "Failed to enqueue scheduled consolidation for character"
                );
            }
        }
    }
    tracing::info!(
        module = MODULE,
        charactersEnqueued = result.characters_enqueued,
        charactersScanned = result.characters_scanned,
        "Scheduled consolidation pass complete"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_mark_takes_the_newest_at_or_before_claim() {
        let marks = vec![(100, 5), (200, 9), (300, 12)];
        assert_eq!(claim_mark(&marks, 250), Some(9));
        assert_eq!(claim_mark(&marks, 200), Some(9));
        assert_eq!(claim_mark(&marks, 99), None);
    }

    #[test]
    fn prune_keeps_the_newest() {
        let mut marks = vec![(0, 1), (10, 2)];
        prune_marks(&mut marks, 10 + MARK_RETENTION_MS * 10);
        assert_eq!(marks, vec![(10, 2)]);
    }
}
