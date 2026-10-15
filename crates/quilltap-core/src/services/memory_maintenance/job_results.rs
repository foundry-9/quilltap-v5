//! R-M — the maintenance job's result, handed from the handler to the
//! completion region: v4's `setJobResult(summary)` (`child-repositories-
//! proxy.ts:167-175`) carried the summary from the job child to the parent's
//! commit hooks over IPC. It is NOT persisted there (`markCompleted(jobId)`
//! takes no result) — "the summary lives only in the in-memory ledger" — so
//! v5 does not use `JobOutcome::Completed(Some(v))`, which would MERGE it into
//! the row's `payload.result`. The handler stashes it here by job id; the ONE
//! completion-region call ([`super::after_job::after_job_completed`]) takes it.
//!
//! The stash also carries what v4's commit hooks derived from the batch's
//! buffered writes, which v5 has no list of (L-2): the digest subjects the run
//! touched (for the Commonplace mirror) and whether it changed rows.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::types::MaintenanceSummary;

/// What a maintenance job hands its completion hooks.
#[derive(Clone, Debug, PartialEq)]
pub struct MaintenanceJobResult {
    pub summary: MaintenanceSummary,
    /// The digest subjects (`aboutCharacterId`s) the run's consolidation
    /// created or revised, holder = `summary.character_id`.
    pub touched_subjects: Vec<String>,
    pub rows_changed: bool,
}

fn stash_map() -> &'static Mutex<HashMap<String, MaintenanceJobResult>> {
    static STASH: OnceLock<Mutex<HashMap<String, MaintenanceJobResult>>> = OnceLock::new();
    STASH.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Hand a finished run's result to its job's completion hooks.
pub fn stash(job_id: &str, result: MaintenanceJobResult) {
    stash_map()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(job_id.to_string(), result);
}

/// Take (and forget) a job's stashed result; `None` when the handler stashed
/// nothing (no `characterId`, deferred, or never ran).
pub fn take(job_id: &str) -> Option<MaintenanceJobResult> {
    stash_map()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(job_id)
}
