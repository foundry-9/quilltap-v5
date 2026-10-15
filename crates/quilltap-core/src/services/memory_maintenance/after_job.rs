//! The maintenance job's commit hooks — v4 `job-dispatcher.ts:246-266`'s
//! success arm (`recordMaintenanceAfterCommit`, `:691-704`) and
//! `commonplace-mirror.ts:204-228`'s `mirrorCommonplaceAfterCommit` (bug 208,
//! `c2795733f`), as ONE call in `job_runner.rs`'s completion region (R-M,
//! §R.10(e)).
//!
//! v4's why, carried: "A maintenance run's outcome goes into the parent's
//! ledger only once its writes have landed." "The Commonplace vault mirror
//! describes committed digests only, so it is written here, after the batch
//! lands — never from the child (bug 208)."
//!
//! L-1 / L-2: v4 reads the touched buckets off the committed write list
//! (`subjectsTouchedByWrites` — created digests from their data, revised ones
//! read back); v5 has no write list, so the run's `touched_subjects` ride the
//! stash ([`super::job_results`]) — the same subjects, in the same order. v4's
//! digest-DELETE hooks (`lookupDeletedDigestBuckets` /
//! `mirrorDeletedDigestsAfterCommit`) are covered by C1-7's
//! `announce_retirement` post-commit refresh (R-AF); their lines (`Revised
//! digest unreadable while collecting mirror subjects; skipped`, `Resolved
//! digest buckets a batch deletes`, `Digest bucket lookup before commit
//! failed; no mirror refresh`, `Deleted-digest lookup skipped before commit`,
//! `Commonplace mirror after digest delete skipped`, `Commonplace mirror
//! refreshed after a committed digest delete`) are NO-PORT (child-transport
//! artefacts). The dispatcher's two outer catches — DEBUG `Maintenance
//! outcome not recorded after job` and WARN `Commonplace mirror skipped after
//! job` (`module: 'jobs:dispatcher'`) — guard a dynamic `import()` and a hook
//! that never throws; neither can fail in-process, so neither has a home.
//! v4's commit-hook watermark (`checkMaintenanceWatermarkAfterCommit`) is
//! R-L's NO-PORT (see [`super::triggers`]).

use serde_json::Value;

use super::job_results;
use super::ledger::record_maintenance_outcome;
use crate::db::background_jobs::BackgroundJob;
use crate::db::runtime::Db;
use crate::services::queue_service::MEMORY_MAINTENANCE;

const MIRROR_MODULE: &str = "memory:commonplace-mirror";

/// v4 `MIRRORING_JOB_TYPES` — the job types whose committed batches the
/// Commonplace mirror follows.
const MIRRORING_JOB_TYPES: [&str; 1] = [MEMORY_MAINTENANCE];

/// The ONE completion-region call (R-M): take the job's stashed result,
/// record it in the ledger (v4 `recordMaintenanceAfterCommit` — a
/// non-maintenance job, or one that stashed nothing, records nothing), then
/// run the mirror hook. A non-maintenance job is a no-op.
pub async fn after_job_completed(db: &Db, job: &BackgroundJob) {
    if job.job_type != MEMORY_MAINTENANCE {
        return;
    }
    let stashed = job_results::take(&job.id);
    if let Some(result) = &stashed {
        record_maintenance_outcome(&result.summary);
    }
    let payload: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
    let touched: Vec<(String, Vec<String>)> = stashed
        .filter(|r| !r.touched_subjects.is_empty())
        .map(|r| vec![(r.summary.character_id.clone(), r.touched_subjects)])
        .unwrap_or_default();
    mirror_commonplace_after_job(db, &job.job_type, &payload, &touched).await;
}

/// v4 `mirrorCommonplaceAfterCommit(jobType, payload, writes)`: "once a
/// maintenance job's batch has committed, mirror every bucket it touched.
/// Never throws." A non-mirroring type or `payload.dryRun === true` returns
/// at once; nothing touched → DEBUG `Committed maintenance batch touched no
/// mirrored bucket {module, jobType}`; else each holder's subjects through
/// C1-7's `mirror_commonplace_for_subjects` at one `updatedAt`. v4's catch
/// (WARN `Commonplace mirror after commit failed (non-fatal) {jobType,
/// error}`) guards `subjectsTouchedByWrites` + the mirror, neither of which
/// fails in v5 (the stash replaces the read, the mirror never fails) — no
/// home.
pub async fn mirror_commonplace_after_job(
    db: &Db,
    job_type: &str,
    payload: &Value,
    touched: &[(String, Vec<String>)],
) {
    if !MIRRORING_JOB_TYPES.contains(&job_type) {
        return;
    }
    if payload.get("dryRun") == Some(&Value::Bool(true)) {
        return;
    }
    if touched.is_empty() {
        tracing::debug!(
            module = MIRROR_MODULE,
            jobType = %job_type,
            "Committed maintenance batch touched no mirrored bucket"
        );
        return;
    }
    let updated_at = crate::clock::now_iso();
    for (holder_id, subjects) in touched {
        crate::services::commonplace_mirror::mirror_commonplace_for_subjects(
            db,
            holder_id,
            subjects,
            Some(&updated_at),
        )
        .await;
    }
}
