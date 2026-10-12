//! The `MEMORY_CONSOLIDATION` job handler — v4
//! `lib/background-jobs/handlers/memory-consolidation.ts` (`d58548051`,
//! byte-identical at the `01a83539d` pin).
//!
//! One job consolidates one character. Automatic triggers (`scheduled`,
//! `watermark`) bail when consolidation is disabled; a `manual` job runs
//! regardless (the operator asked). The run gets a wall-clock budget
//! ([`CONSOLIDATION_JOB_TIME_BUDGET_MS`], 7 minutes) that keeps it inside the
//! runner's 10-minute stuck-job window (`job_runner::STUCK_JOB_TIMEOUT_MINUTES`):
//! a run stops issuing calls after this long and commits what it has, leaving
//! the rest of the backlog for the next pass.
//!
//! A lost cluster (a failed, timed-out or invalid call) does not fail the job:
//! failing it would roll back every other cluster's digests with it. The lost
//! cluster's rows stay unconsidered and the next run retries them. A failure
//! of the run itself (the corpus cannot be read, the plan cannot be committed)
//! DOES fail the job.
//!
//! The run is `in_job` (P4.D271 R-C): the runner's completion hooks publish
//! `memories` and invalidate the frozen archive once each.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::db::background_jobs::BackgroundJob;
use crate::db::instance_settings::get_memory_consolidation_settings;
use crate::db::runtime::Db;
use crate::services::consolidation::{
    ConsolidationRunner, ConsolidationTrigger, RunConsolidationOptions,
};
use crate::services::job_runner::{JobFuture, JobHandler, JobOutcome};
use crate::services::queue_service::MemoryConsolidationPayload;

/// Wall-clock budget for one job's model calls (7 minutes).
pub const CONSOLIDATION_JOB_TIME_BUDGET_MS: i64 = 7 * 60 * 1000;

const MODULE: &str = "jobs:memory-consolidation";

/// v4 `handleMemoryConsolidation`.
pub async fn handle_memory_consolidation(
    db: &Db,
    runner: &dyn ConsolidationRunner,
    job: &BackgroundJob,
) -> JobOutcome {
    let raw: Value = serde_json::from_str(&job.payload).unwrap_or(Value::Null);
    let Some(payload) = MemoryConsolidationPayload::decode(&raw) else {
        tracing::warn!(
            module = MODULE,
            jobId = %job.id,
            "Consolidation job has no characterId; nothing to do"
        );
        return JobOutcome::Completed(None);
    };
    // v4 `payload.trigger ?? 'manual'`; an unrecognized string is carried as
    // v4 carries it (only `manual` skips the enabled gate).
    let trigger_str = payload
        .trigger
        .clone()
        .unwrap_or_else(|| "manual".to_string());
    let trigger = ConsolidationTrigger::parse(&trigger_str).unwrap_or(ConsolidationTrigger::Manual);

    if trigger_str != "manual" {
        let enabled = db
            .read_main(|c| Ok(get_memory_consolidation_settings(c)))
            .map(|s| s.enabled)
            .unwrap_or(false);
        if !enabled {
            tracing::debug!(
                module = MODULE,
                jobId = %job.id,
                characterId = %payload.character_id,
                trigger = %trigger_str,
                "Consolidation disabled; automatic job exits without running"
            );
            return JobOutcome::Completed(None);
        }
    }

    let report = match runner
        .run(
            db,
            &payload.character_id,
            RunConsolidationOptions {
                dry_run: payload.dry_run == Some(true),
                max_clusters_per_run: payload.max_clusters_per_run,
                user_id: Some(job.user_id.clone()),
                time_budget_ms: Some(CONSOLIDATION_JOB_TIME_BUDGET_MS),
                trigger,
                in_job: true,
                ..Default::default()
            },
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return JobOutcome::Failed(crate::db::fallback::error_text(&e)),
    };

    if report.dry_run {
        // A job has no result slot, so a dry run's report goes to the log. The
        // manual surfaces call the run in-process for the full report.
        for cluster in &report.clusters {
            let digests: Vec<Value> = cluster
                .digests
                .iter()
                .map(|d| json!({ "action": d.action, "summary": d.summary, "members": d.member_ids.len() }))
                .collect();
            let bucket = format!(
                "{}:{}",
                cluster.bucket.kind.as_str(),
                cluster.bucket.subject_name
            );
            let existing = cluster
                .existing_digest_id
                .clone()
                .unwrap_or_else(|| "null".to_string());
            let digests_json = serde_json::to_string(&digests).unwrap_or_default();
            match &cluster.error {
                Some(error) => tracing::info!(
                    module = MODULE,
                    jobId = %job.id,
                    characterId = %payload.character_id,
                    bucket = %bucket,
                    clusterKind = cluster.cluster_kind.as_str(),
                    status = cluster.status,
                    members = cluster.member_ids.len(),
                    existingDigestId = %existing,
                    digestsJson = %digests_json,
                    keepStandalone = cluster.keep_standalone.len(),
                    contradictions = cluster.contradictions.len(),
                    error = %error,
                    "[Dry run] Consolidation cluster"
                ),
                None => tracing::info!(
                    module = MODULE,
                    jobId = %job.id,
                    characterId = %payload.character_id,
                    bucket = %bucket,
                    clusterKind = cluster.cluster_kind.as_str(),
                    status = cluster.status,
                    members = cluster.member_ids.len(),
                    existingDigestId = %existing,
                    digestsJson = %digests_json,
                    keepStandalone = cluster.keep_standalone.len(),
                    contradictions = cluster.contradictions.len(),
                    "[Dry run] Consolidation cluster"
                ),
            }
        }
    }

    let stats = &report.stats;
    match report.skipped_reason {
        Some(reason) => tracing::info!(
            module = MODULE,
            jobId = %job.id,
            characterId = %payload.character_id,
            trigger = %trigger_str,
            dryRun = report.dry_run,
            skippedReason = reason,
            digestsCreated = stats.digests_created,
            digestsUpdated = stats.digests_updated,
            membersSuperseded = stats.members_superseded,
            clustersFailed = stats.clusters_failed,
            clustersDeferred = stats.clusters_deferred,
            durationMs = stats.duration_ms,
            "Consolidation job complete"
        ),
        None => tracing::info!(
            module = MODULE,
            jobId = %job.id,
            characterId = %payload.character_id,
            trigger = %trigger_str,
            dryRun = report.dry_run,
            digestsCreated = stats.digests_created,
            digestsUpdated = stats.digests_updated,
            membersSuperseded = stats.members_superseded,
            clustersFailed = stats.clusters_failed,
            clustersDeferred = stats.clusters_deferred,
            durationMs = stats.duration_ms,
            "Consolidation job complete"
        ),
    }
    JobOutcome::Completed(None)
}

/// The registered `MEMORY_CONSOLIDATION` handler over a [`ConsolidationRunner`].
pub struct MemoryConsolidationJobHandler {
    pub runner: Arc<dyn ConsolidationRunner>,
}

impl JobHandler for MemoryConsolidationJobHandler {
    fn handle<'a>(&'a self, db: &'a Db, job: &'a BackgroundJob) -> JobFuture<'a> {
        Box::pin(handle_memory_consolidation(db, self.runner.as_ref(), job))
    }
}
