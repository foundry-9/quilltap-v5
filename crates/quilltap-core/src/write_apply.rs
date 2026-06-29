//! Port of the partitioned write *applier* in
//! lib/background-jobs/host/job-dispatcher.ts — `applyWritesUnsafe`,
//! `applyPartition`, `applySecondaryBestEffort`, and `applyFolderCreateIdempotent`.
//!
//! Where [`crate::write_partition`] holds the pure classification/partition/remap
//! leaves, this module holds the **orchestration** that sequences them: each
//! partition (main / mount-index / llm-logs) commits in its OWN transaction on
//! its OWN connection, with the per-job-type ordering and failure policy and the
//! concurrent-folder-create reconcile. These are the architectural invariants
//! CLAUDE.md says to keep from v4 — correctness properties, not Node workarounds.
//!
//! ## The host seam
//!
//! v4's applier is wired to module singletons (`getRawDatabase()`,
//! `getRepositories()`) and is unit-tested with fake DBs + recording repos — the
//! apply path is *orchestration*; the actual row mutations are delegated to the
//! repos (each tier-2-verified on its own). The native port mirrors that: the
//! engine is generic over an [`ApplyHost`] that owns the three connections and
//! the repo dispatch. Production wires real connections/repos; the differential
//! harness wires a recorder and diffs the resulting trace against v4's real
//! `applyWritesUnsafe` driven over the same corpus.
//!
//! ## Deferred (documented, not silently dropped)
//!
//! - `__finalizeFile` (the staged-file rename inside the main transaction, with
//!   undo-on-rollback) — a filesystem step, orthogonal to the partition
//!   orchestration; lands with the file-write path. The corpus excludes it.
//! - `cleanupStagingDirs` / `dispatchInvalidations` — post-commit side effects
//!   (fs cleanup, cache invalidation), not DB-apply correctness.

use std::collections::HashMap;

use serde_json::Value;

use crate::write_partition::{
    is_main_primary_job_type, is_unique_constraint_error, partition_writes, rewrite_folder_refs,
    ChildWritePayload, WriteDbTarget, DOC_MOUNT_FOLDER_CREATE,
};

/// An error raised by a repo dispatch, a connection op, or the reconcile lookup.
/// Mirrors v4's thrown `Error` (optionally carrying a SQLite `code`), enough for
/// [`ApplyError::is_unique_constraint`] to classify it.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyError {
    pub message: String,
    pub code: Option<String>,
}

impl ApplyError {
    /// An error with only a message (no SQLite code).
    pub fn msg(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
        }
    }

    /// Whether this is a SQLite uniqueness/PK violation — the signature of a
    /// concurrent folder create losing the race. Reuses the oracle-verified
    /// [`is_unique_constraint_error`] over a `{code,message}` value.
    pub fn is_unique_constraint(&self) -> bool {
        let mut m = serde_json::Map::new();
        if let Some(c) = &self.code {
            m.insert("code".into(), Value::String(c.clone()));
        }
        m.insert("message".into(), Value::String(self.message.clone()));
        is_unique_constraint_error(&Value::Object(m))
    }
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApplyError {}

/// The seam the applier drives: the three partition connections plus repo
/// dispatch and the folder-reconcile lookup. Production wires real
/// connections/repos; the harness wires a recorder.
pub trait ApplyHost {
    /// Whether the connection backing `partition` is initialized. A partition
    /// with writes but no connection is a hard error (v4's "connection is not
    /// initialized").
    fn conn_available(&self, partition: WriteDbTarget) -> bool;

    /// Run a transaction-control statement (`BEGIN IMMEDIATE` / `COMMIT` /
    /// `ROLLBACK`) on `partition`'s connection.
    fn conn_exec(&mut self, partition: WriteDbTarget, sql: &str) -> Result<(), ApplyError>;

    /// Apply one buffered write by dispatching its dotted method to the repo
    /// layer (v4's `applyRepositoryWrite`). The payload is already
    /// folder-ref-rewritten for the mount partition.
    fn dispatch(&mut self, write: &ChildWritePayload) -> Result<(), ApplyError>;

    /// Resolve an already-committed `doc_mount_folders` row by its identifying
    /// `(mountPointId, path)` (v4's `findByMountPointAndPath`), for the
    /// concurrent-create reconcile. `None` => no such row.
    fn find_folder(
        &mut self,
        mount_point_id: &str,
        path: &str,
    ) -> Result<Option<String>, ApplyError>;
}

/// Apply a job's buffered writes, partitioned by target database. Mirrors v4's
/// `applyWritesUnsafe`: main-primary jobs commit main first and authoritatively,
/// then apply secondaries best-effort; every other (idempotent) job applies
/// secondaries first so a secondary failure prevents the main commit. The
/// post-commit `cleanupStagingDirs` / `dispatchInvalidations` side effects are
/// deferred (see the module note).
pub fn apply_writes(
    host: &mut dyn ApplyHost,
    job_id: &str,
    writes: &[ChildWritePayload],
    job_type: Option<&str>,
) -> Result<(), ApplyError> {
    let parts = partition_writes(writes);

    if is_main_primary_job_type(job_type) {
        apply_partition(host, WriteDbTarget::Main, &parts.main, job_id)?;
        apply_secondary_best_effort(host, WriteDbTarget::MountIndex, &parts.mount_index, job_id);
        apply_secondary_best_effort(host, WriteDbTarget::LlmLogs, &parts.llm_logs, job_id);
    } else {
        apply_partition(host, WriteDbTarget::MountIndex, &parts.mount_index, job_id)?;
        apply_partition(host, WriteDbTarget::LlmLogs, &parts.llm_logs, job_id)?;
        apply_partition(host, WriteDbTarget::Main, &parts.main, job_id)?;
    }
    Ok(())
}

/// Apply one partition's writes inside a single hand-driven transaction. Throws
/// on any failure (after rolling the partition back). No-op for an empty
/// partition. `BEGIN IMMEDIATE` is taken up front (outside the rollback scope, as
/// in v4) so lock contention surfaces early.
fn apply_partition(
    host: &mut dyn ApplyHost,
    partition: WriteDbTarget,
    writes: &[ChildWritePayload],
    job_id: &str,
) -> Result<(), ApplyError> {
    if writes.is_empty() {
        return Ok(());
    }
    if !host.conn_available(partition) {
        return Err(ApplyError::msg(format!(
            "Cannot apply {} writes for job {}: database connection is not initialized",
            partition.as_str(),
            job_id
        )));
    }

    host.conn_exec(partition, "BEGIN IMMEDIATE")?;
    match apply_partition_body(host, partition, writes, job_id) {
        Ok(()) => Ok(()),
        Err(e) => {
            // ROLLBACK is best-effort (may already be rolled back).
            let _ = host.conn_exec(partition, "ROLLBACK");
            Err(e)
        }
    }
}

/// The transaction body: dispatch each write (rewriting folder refs and handling
/// the idempotent folder create on the mount partition), then `COMMIT`. Any error
/// here triggers the caller's `ROLLBACK`.
fn apply_partition_body(
    host: &mut dyn ApplyHost,
    partition: WriteDbTarget,
    writes: &[ChildWritePayload],
    job_id: &str,
) -> Result<(), ApplyError> {
    let is_mount = partition == WriteDbTarget::MountIndex;
    // bufferedFolderId -> existing folderId, populated when a concurrent folder
    // create is reconciled to an already-committed row (mount-index only).
    let mut folder_remap: HashMap<String, String> = HashMap::new();

    for raw in writes {
        // Redirect any folder reference an earlier same-batch create reconciled
        // to an existing row (no-op when nothing has been remapped / not mount).
        let w = if is_mount {
            rewrite_folder_refs(raw, &folder_remap)
        } else {
            raw.clone()
        };

        if is_mount && w.method == DOC_MOUNT_FOLDER_CREATE {
            apply_folder_create_idempotent(host, &w, &mut folder_remap, job_id)?;
            continue;
        }

        host.dispatch(&w)?;
    }

    host.conn_exec(partition, "COMMIT")?;
    Ok(())
}

/// Apply a secondary (non-main) partition best-effort: a failure is rolled back
/// inside [`apply_partition`], then logged and swallowed so the already-committed
/// main partition (the chat turn) survives. Only reached for main-primary jobs.
fn apply_secondary_best_effort(
    host: &mut dyn ApplyHost,
    partition: WriteDbTarget,
    writes: &[ChildWritePayload],
    job_id: &str,
) {
    if writes.is_empty() {
        return;
    }
    // The error is intentionally dropped — the committed main partition survives
    // the lost secondary effect. (v4 logs it; logging lands with the host wiring.)
    let _ = apply_partition(host, partition, writes, job_id);
}

/// Apply a `docMountFolders.create`, tolerating the rare cross-job concurrent
/// create: if another job committed the same `(mountPointId, path)` first, the
/// INSERT hits the unique index; because applies are serialized, that row is
/// visible, so we resolve to it and remap the discarded buffered folder id for
/// the rest of this batch. SQLite's ABORT conflict resolution rolls back only the
/// offending statement, so the surrounding transaction stays usable.
fn apply_folder_create_idempotent(
    host: &mut dyn ApplyHost,
    write: &ChildWritePayload,
    folder_remap: &mut HashMap<String, String>,
    _job_id: &str,
) -> Result<(), ApplyError> {
    let err = match host.dispatch(write) {
        Ok(()) => return Ok(()), // created fresh
        Err(e) => e,
    };

    if !err.is_unique_constraint() {
        return Err(err);
    }

    let data = write.args.first();
    let options = write.args.get(1);
    let buffered_id = options
        .and_then(|o| o.get("id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mount_point_id = data
        .and_then(|d| d.get("mountPointId"))
        .and_then(Value::as_str);
    let path = data.and_then(|d| d.get("path")).and_then(Value::as_str);

    let (mount_point_id, path) = match (mount_point_id, path) {
        (Some(m), Some(p)) => (m, p),
        // Can't reconcile without the identifying (mountPointId, path).
        _ => return Err(err),
    };

    match host.find_folder(mount_point_id, path)? {
        // Unique conflict but no matching row — genuine corruption; surface it.
        None => Err(err),
        Some(existing_id) => {
            if let Some(buffered) = buffered_id {
                if buffered != existing_id {
                    folder_remap.insert(buffered, existing_id);
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal host: every connection available, dispatch always succeeds, no
    /// reconcile rows. Records the partition each transaction-control op hit and
    /// the order of dispatched methods.
    #[derive(Default)]
    struct OkHost {
        exec: Vec<(WriteDbTarget, String)>,
        dispatched: Vec<String>,
    }
    impl ApplyHost for OkHost {
        fn conn_available(&self, _p: WriteDbTarget) -> bool {
            true
        }
        fn conn_exec(&mut self, p: WriteDbTarget, sql: &str) -> Result<(), ApplyError> {
            self.exec.push((p, sql.to_string()));
            Ok(())
        }
        fn dispatch(&mut self, w: &ChildWritePayload) -> Result<(), ApplyError> {
            self.dispatched.push(w.method.clone());
            Ok(())
        }
        fn find_folder(&mut self, _m: &str, _p: &str) -> Result<Option<String>, ApplyError> {
            Ok(None)
        }
    }

    fn write(method: &str) -> ChildWritePayload {
        ChildWritePayload {
            method: method.to_string(),
            args: vec![json!({})],
        }
    }

    #[test]
    fn idempotent_orders_secondaries_before_main() {
        let mut host = OkHost::default();
        let writes = vec![
            write("chats.update"),
            write("docMountChunks.updateEmbedding"),
        ];
        apply_writes(&mut host, "j", &writes, Some("EMBEDDING_GENERATE")).unwrap();
        // Mount partition committed before main; both opened a transaction.
        assert_eq!(
            host.exec,
            vec![
                (WriteDbTarget::MountIndex, "BEGIN IMMEDIATE".into()),
                (WriteDbTarget::MountIndex, "COMMIT".into()),
                (WriteDbTarget::Main, "BEGIN IMMEDIATE".into()),
                (WriteDbTarget::Main, "COMMIT".into()),
            ]
        );
        assert_eq!(
            host.dispatched,
            vec!["docMountChunks.updateEmbedding", "chats.update"]
        );
    }

    #[test]
    fn main_primary_commits_main_first() {
        let mut host = OkHost::default();
        let writes = vec![
            write("chats.addMessage"),
            write("docMountChunks.updateEmbedding"),
        ];
        apply_writes(&mut host, "j", &writes, Some("AUTONOMOUS_ROOM_TURN")).unwrap();
        // Main partition is the first to open/commit a transaction.
        assert_eq!(
            host.exec[0],
            (WriteDbTarget::Main, "BEGIN IMMEDIATE".into())
        );
        assert_eq!(host.exec[1], (WriteDbTarget::Main, "COMMIT".into()));
    }

    #[test]
    fn empty_partition_is_a_noop() {
        let mut host = OkHost::default();
        apply_writes(&mut host, "j", &[], None).unwrap();
        assert!(host.exec.is_empty());
        assert!(host.dispatched.is_empty());
    }

    #[test]
    fn unique_constraint_classification_reused() {
        assert!(ApplyError {
            message: "x".into(),
            code: Some("SQLITE_CONSTRAINT_UNIQUE".into()),
        }
        .is_unique_constraint());
        assert!(ApplyError::msg("UNIQUE constraint failed: t.col").is_unique_constraint());
        assert!(!ApplyError::msg("some other error").is_unique_constraint());
    }
}
