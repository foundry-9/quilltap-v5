//! The background-job enqueue helpers the memory gate's watermark path needs
//! (v4 `lib/background-jobs/queue-service.ts`, the `enqueueJob` +
//! `enqueueMemoryHousekeeping` slice). The rest of the queue service (the
//! per-task enqueue family, stats, the processor) lands with the job-runner
//! unit.
//!
//! ## Deferred (tracked, out of scope for this unit)
//!
//!   - **`ensureProcessorRunning()`** — v4 auto-starts the in-process job
//!     processor on every enqueue. The Rust job runner is a later Phase-3
//!     unit; enqueue-only leaves the row PENDING (the differential's oracle
//!     mocks the v4 auto-start to a no-op to match).

use serde_json::Value;

use crate::clock::now_iso;
use crate::db::background_jobs::{BjCreate, CreateOptions};
use crate::db::runtime::Db;
use crate::db::DbError;

/// v4 `enqueueJob`: mint and persist a PENDING background job. Returns the job
/// id.
pub async fn enqueue_job(
    db: &Db,
    user_id: &str,
    job_type: &str,
    payload: Value,
    max_attempts: f64,
) -> Result<String, DbError> {
    let now = now_iso();
    let id = uuid::Uuid::new_v4().to_string();
    let create = BjCreate {
        user_id: user_id.to_string(),
        job_type: job_type.to_string(),
        status: Some("PENDING".to_string()),
        payload,
        priority: 0.0,
        attempts: 0.0,
        max_attempts,
        last_error: None,
        scheduled_at: now.clone(),
        started_at: None,
        completed_at: None,
    };
    let opts = CreateOptions {
        id: id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    db.write(move |writers| writers.main().background_jobs().create(&create, &opts))
        .await?;
    // Deferred: `ensureProcessorRunning()` — the job runner is a later unit.
    Ok(id)
}

/// v4 `enqueueMemoryHousekeeping`: de-dupe against in-flight
/// `MEMORY_HOUSEKEEPING` jobs for the same (userId, characterId), then enqueue
/// with `maxAttempts: 1` (housekeeping is retry-hostile — the daily scheduler
/// re-enqueues anyway). Returns the existing or new job id.
pub async fn enqueue_memory_housekeeping(
    db: &Db,
    user_id: &str,
    payload: Value,
) -> Result<String, DbError> {
    // De-dupe against in-flight jobs. v4 wraps the check in a try/catch and
    // falls through to enqueue on failure — double work beats none.
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        let our_char = payload.get("characterId").and_then(Value::as_str);
        let existing = jobs.iter().find(|j| {
            if j.job_type != "MEMORY_HOUSEKEEPING" {
                return false;
            }
            let job_char: Option<Value> = serde_json::from_str::<Value>(&j.payload)
                .ok()
                .and_then(|p| p.get("characterId").cloned());
            job_char.as_ref().and_then(Value::as_str) == our_char
        });
        if let Some(existing) = existing {
            return Ok(existing.id.clone());
        }
    }

    enqueue_job(db, user_id, "MEMORY_HOUSEKEEPING", payload, 1.0).await
}

/// v4 `enqueueTitleUpdate` (`lib/background-jobs/queue-service.ts`): enqueue a
/// `TITLE_UPDATE` job, de-duping on `chatId`. If a PENDING or PROCESSING
/// `TITLE_UPDATE` job already exists for the same `chatId`, this is a no-op that
/// returns the existing job id (multiple finalizer firings at the same
/// interchange checkpoint fold into one pending job). `maxAttempts` defaults to 3
/// (v4 `options?.maxAttempts ?? 3`; the differential passes no options). The
/// `skipDedupCheck` option is not modeled — the context-summary caller never sets
/// it. A dedupe-lookup failure falls through and enqueues anyway (v4 warns).
pub async fn enqueue_title_update(
    db: &Db,
    user_id: &str,
    chat_id: &str,
    payload: Value,
) -> Result<String, DbError> {
    let uid = user_id.to_string();
    let in_flight = db.read_main(|conn| {
        let repo = crate::db::background_jobs::BackgroundJobsRepository::new(conn);
        let mut jobs = repo.find_by_user_id(&uid, Some("PENDING"))?;
        jobs.extend(repo.find_by_user_id(&uid, Some("PROCESSING"))?);
        Ok(jobs)
    });
    if let Ok(jobs) = in_flight {
        let existing = jobs.iter().find(|j| {
            if j.job_type != "TITLE_UPDATE" {
                return false;
            }
            let job_chat: Option<Value> = serde_json::from_str::<Value>(&j.payload)
                .ok()
                .and_then(|p| p.get("chatId").cloned());
            job_chat.as_ref().and_then(Value::as_str) == Some(chat_id)
        });
        if let Some(existing) = existing {
            return Ok(existing.id.clone());
        }
    }

    enqueue_job(db, user_id, "TITLE_UPDATE", payload, 3.0).await
}
