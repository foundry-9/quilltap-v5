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
//! ## The filesystem + post-commit seam
//!
//! Three effects are pure orchestration over a filesystem/cache boundary, so —
//! like the repo dispatch — they route through [`ApplyHost`] (production wires
//! real fs/cache ops; the harness records them and diffs the trace against v4):
//!
//! - `__finalizeFile` — the staged-file rename performed *inside* the main-DB
//!   transaction loop (`ensureDir(dirname(final))` + `rename(staging → final)`),
//!   tracked so a later failure in the same partition **undoes the renames** in
//!   reverse before rethrowing. The engine computes the paths (the pure
//!   [`path_dirname`]); the host performs the fs op.
//! - `cleanupStagingDirs` — post-commit, drop the per-job `.staging/<jobId>`
//!   shell derived from the first `__finalizeFile` (the pure [`find_staging_root`]).
//! - `dispatchInvalidations` — post-commit, fire the *deduped, ordered* vector-store
//!   / mount-cache invalidation targets (the pure [`collect_invalidations`]). The
//!   host owns the child IPC + local cache eviction (best-effort effects).

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::write_partition::{
    is_main_primary_job_type, is_unique_constraint_error, partition_writes, rewrite_folder_refs,
    ChildWritePayload, WriteDbTarget, DOC_MOUNT_FOLDER_CREATE, FINALIZE_FILE,
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

    /// `__finalizeFile`: ensure `final_dir` exists, then rename `staging_path` →
    /// `final_path` (v4's `ensureDirSync` + `fs.renameSync`), inside the main
    /// transaction. An `Err` triggers the caller's ROLLBACK + undo of any earlier
    /// finalize in this partition.
    fn finalize_file(
        &mut self,
        final_dir: &str,
        staging_path: &str,
        final_path: &str,
    ) -> Result<(), ApplyError>;

    /// Undo a completed finalize on rollback: rename `final_path` → `staging_path`
    /// (v4's reverse `fs.renameSync(to, from)`). Best-effort — infallible here
    /// (v4 swallows the error).
    fn undo_finalize(&mut self, final_path: &str, staging_path: &str);

    /// Post-commit: remove the per-job staging root directory (v4's
    /// `fs.rmSync(root, {recursive, force})`). Best-effort.
    fn cleanup_staging_dir(&mut self, staging_root: &str);

    /// Post-commit: fire the deduped, ordered cache invalidations (v4's
    /// `notifyChild` + local eviction). Both key lists are first-seen order.
    fn dispatch_invalidations(&mut self, vector_store_keys: &[String], mount_point_keys: &[String]);
}

/// Apply a job's buffered writes, partitioned by target database. Mirrors v4's
/// `applyWritesUnsafe`: main-primary jobs commit main first and authoritatively,
/// then apply secondaries best-effort; every other (idempotent) job applies
/// secondaries first so a secondary failure prevents the main commit.
///
/// After every partition commits, the post-commit side effects fire:
/// [`cleanup_staging_dirs`] drops the per-job `.staging/<jobId>` shell, then
/// [`dispatch_invalidations`] fires the deduped cache invalidations. Both run
/// only on success — a partition throw short-circuits (via `?`) before them, so
/// a failed batch leaves its staging dir + caches untouched (as in v4).
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

    cleanup_staging_dirs(host, writes, job_id);
    dispatch_invalidations(host, writes);
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
    // Staged file renames performed in this partition's loop: (staging, final).
    // On rollback they are undone in reverse (v4's `stagedRenames.reverse()`).
    let mut staged: Vec<(String, String)> = Vec::new();
    match apply_partition_body(host, partition, writes, job_id, &mut staged) {
        Ok(()) => Ok(()),
        Err(e) => {
            // ROLLBACK is best-effort (may already be rolled back).
            let _ = host.conn_exec(partition, "ROLLBACK");
            // Undo any file renames that completed before the throw, in reverse.
            for (staging, final_path) in staged.iter().rev() {
                host.undo_finalize(final_path, staging);
            }
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
    staged: &mut Vec<(String, String)>,
) -> Result<(), ApplyError> {
    let is_mount = partition == WriteDbTarget::MountIndex;
    // bufferedFolderId -> existing folderId, populated when a concurrent folder
    // create is reconciled to an already-committed row (mount-index only).
    let mut folder_remap: HashMap<String, String> = HashMap::new();

    for raw in writes {
        // The staged-file finalize is a built-in intercepted before any repo
        // dispatch (v4 checks `raw.method === '__finalizeFile'` first). It only
        // ever lands in the Main partition (see `classify_write_target`).
        if raw.method == FINALIZE_FILE {
            let (staging_path, final_path) = finalize_args(raw);
            let final_dir = path_dirname(final_path);
            // Record the rename only after it lands, so a finalize that itself
            // fails leaves nothing to undo (the rename never happened).
            host.finalize_file(&final_dir, staging_path, final_path)?;
            staged.push((staging_path.to_string(), final_path.to_string()));
            continue;
        }

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

// ============================================================================
// __finalizeFile helpers
// ============================================================================

/// Pull `(stagingPath, finalPath)` out of a `__finalizeFile` write's `args[0]`.
/// The corpus always supplies both; a missing field degrades to `""` (v4 would
/// throw a TypeError, never reached).
fn finalize_args(w: &ChildWritePayload) -> (&str, &str) {
    let a0 = w.args.first();
    let staging = a0
        .and_then(|v| v.get("stagingPath"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let final_path = a0
        .and_then(|v| v.get("finalPath"))
        .and_then(Value::as_str)
        .unwrap_or("");
    (staging, final_path)
}

/// Faithful port of Node's `path.posix.dirname` (the applier runs on
/// macOS/Linux, so `path` is posix). Returns the directory portion of a path:
/// `.` for a rootless bare name, `/` for a root-only path, else everything up to
/// (not including) the final non-trailing slash. Byte-indexed, which is
/// equivalent to Node's UTF-16 indexing here because every boundary is an ASCII
/// `/`.
fn path_dirname(path: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.is_empty() {
        return ".".to_string();
    }
    let has_root = bytes[0] == b'/';
    let mut end: isize = -1;
    let mut matched_slash = true;
    let mut i = bytes.len() as isize - 1;
    while i >= 1 {
        if bytes[i as usize] == b'/' {
            if !matched_slash {
                end = i;
                break;
            }
        } else {
            matched_slash = false;
        }
        i -= 1;
    }
    if end == -1 {
        return if has_root { "/" } else { "." }.to_string();
    }
    if has_root && end == 1 {
        return "//".to_string();
    }
    path[..end as usize].to_string()
}

/// Faithful port of v4's `findStagingRoot`: locate `.staging/<jobId>` within the
/// staging path and return the prefix through it (the per-job staging root), or
/// `None` when the marker isn't present. `path.sep` on the run platform is `/`.
fn find_staging_root(staging_path: &str, job_id: &str) -> Option<String> {
    let needle = format!(".staging{}{}", std::path::MAIN_SEPARATOR, job_id);
    let idx = staging_path.find(&needle)?;
    let end = idx + needle.len();
    Some(staging_path[..end].to_string())
}

/// Post-commit: drop the per-job staging directory derived from the first
/// `__finalizeFile` whose staging path carries the `.staging/<jobId>` marker
/// (v4's `cleanupStagingDirs`). A `__finalizeFile` without the marker is skipped
/// (v4's `continue`); the first that yields a root cleans up and returns.
fn cleanup_staging_dirs(host: &mut dyn ApplyHost, writes: &[ChildWritePayload], job_id: &str) {
    for w in writes {
        if w.method != FINALIZE_FILE {
            continue;
        }
        let (staging_path, _) = finalize_args(w);
        let Some(root) = find_staging_root(staging_path, job_id) else {
            continue;
        };
        host.cleanup_staging_dir(&root);
        return;
    }
}

// ============================================================================
// Cache invalidation
// ============================================================================

/// Repo methods whose success invalidates a character's vector store (v4's
/// `WRITES_INVALIDATING_VECTOR_STORE`).
const WRITES_INVALIDATING_VECTOR_STORE: &[&str] = &[
    "vectorIndices.deleteStore",
    "vectorIndices.addEntry",
    "vectorIndices.updateEntryEmbedding",
    "vectorIndices.saveMeta",
    "memories.updateForCharacter",
    "memories.delete",
    "memories.create",
    "memories.upsert",
];

/// Repo methods whose success invalidates a mount point's chunk cache (v4's
/// `WRITES_INVALIDATING_MOUNT_CACHE`).
const WRITES_INVALIDATING_MOUNT_CACHE: &[&str] = &[
    "docMountChunks.upsert",
    "docMountChunks.delete",
    "docMountChunks.deleteByMountPointId",
];

/// v4's `extractCharacterId`: the character id is either the string `args[0]`
/// (non-empty) or `args[0].characterId`. Returns `None` otherwise.
fn extract_character_id(w: &ChildWritePayload) -> Option<String> {
    match w.args.first() {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Object(o)) => match o.get("characterId") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// v4's `extractMountPointId`: the mount-point id is either the string `args[0]`
/// (non-empty) or `args[0].mountPointId`. Returns `None` otherwise.
fn extract_mount_point_id(w: &ChildWritePayload) -> Option<String> {
    match w.args.first() {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Object(o)) => match o.get("mountPointId") {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Collect the deduped invalidation targets across the batch (v4's two `Set`s in
/// `dispatchInvalidations`): the vector-store character ids and the mount-cache
/// mount-point ids, each in first-seen order. An empty-string id is falsy in v4's
/// `if (id && SET.has(method))` guard, so it never becomes a key.
fn collect_invalidations(writes: &[ChildWritePayload]) -> (Vec<String>, Vec<String>) {
    let mut vector_keys: Vec<String> = Vec::new();
    let mut vector_seen: HashSet<String> = HashSet::new();
    let mut mount_keys: Vec<String> = Vec::new();
    let mut mount_seen: HashSet<String> = HashSet::new();

    for w in writes {
        if let Some(char_id) = extract_character_id(w) {
            // `id &&` (v4): an empty-string id is falsy, never a key.
            if !char_id.is_empty()
                && WRITES_INVALIDATING_VECTOR_STORE.contains(&w.method.as_str())
                && vector_seen.insert(char_id.clone())
            {
                vector_keys.push(char_id);
            }
        }
        if let Some(mount_id) = extract_mount_point_id(w) {
            if !mount_id.is_empty()
                && WRITES_INVALIDATING_MOUNT_CACHE.contains(&w.method.as_str())
                && mount_seen.insert(mount_id.clone())
            {
                mount_keys.push(mount_id);
            }
        }
    }

    (vector_keys, mount_keys)
}

/// Post-commit: fire the deduped cache invalidations (v4's `dispatchInvalidations`).
/// A no-op when nothing is invalidated (v4's early return).
fn dispatch_invalidations(host: &mut dyn ApplyHost, writes: &[ChildWritePayload]) {
    let (vector_keys, mount_keys) = collect_invalidations(writes);
    if vector_keys.is_empty() && mount_keys.is_empty() {
        return;
    }
    host.dispatch_invalidations(&vector_keys, &mount_keys);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal host: every connection available, no reconcile rows. Records the
    /// partition each transaction-control op hit, dispatched methods, and the
    /// fs/invalidation effects. `fail_on` makes one dispatched method error (to
    /// drive the rollback path).
    #[derive(Default)]
    struct OkHost {
        exec: Vec<(WriteDbTarget, String)>,
        dispatched: Vec<String>,
        fail_on: Option<String>,
        renames: Vec<(String, String)>, // (from, to) across finalize + undo
        mkdirs: Vec<String>,
        cleaned: Vec<String>,
        invalidations: Vec<(String, String)>, // (kind, key)
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
            if self.fail_on.as_deref() == Some(w.method.as_str()) {
                return Err(ApplyError::msg("boom"));
            }
            Ok(())
        }
        fn find_folder(&mut self, _m: &str, _p: &str) -> Result<Option<String>, ApplyError> {
            Ok(None)
        }
        fn finalize_file(
            &mut self,
            final_dir: &str,
            staging_path: &str,
            final_path: &str,
        ) -> Result<(), ApplyError> {
            self.mkdirs.push(final_dir.to_string());
            self.renames
                .push((staging_path.to_string(), final_path.to_string()));
            Ok(())
        }
        fn undo_finalize(&mut self, final_path: &str, staging_path: &str) {
            self.renames
                .push((final_path.to_string(), staging_path.to_string()));
        }
        fn cleanup_staging_dir(&mut self, staging_root: &str) {
            self.cleaned.push(staging_root.to_string());
        }
        fn dispatch_invalidations(&mut self, vector_keys: &[String], mount_keys: &[String]) {
            for k in vector_keys {
                self.invalidations
                    .push(("vectorStore".to_string(), k.clone()));
            }
            for k in mount_keys {
                self.invalidations
                    .push(("mountPoint".to_string(), k.clone()));
            }
        }
    }

    fn write(method: &str) -> ChildWritePayload {
        ChildWritePayload {
            method: method.to_string(),
            args: vec![json!({})],
        }
    }

    fn finalize(staging: &str, final_path: &str) -> ChildWritePayload {
        ChildWritePayload {
            method: FINALIZE_FILE.to_string(),
            args: vec![json!({ "stagingPath": staging, "finalPath": final_path })],
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

    #[test]
    fn path_dirname_matches_node_posix() {
        assert_eq!(
            path_dirname("/data/files/store/abc/x.md"),
            "/data/files/store/abc"
        );
        assert_eq!(path_dirname("/x.md"), "/");
        assert_eq!(path_dirname("x.md"), ".");
        assert_eq!(path_dirname(""), ".");
        assert_eq!(path_dirname("/"), "/");
        // trailing slash ignored, like Node
        assert_eq!(path_dirname("/a/b/"), "/a");
    }

    #[test]
    fn find_staging_root_slices_through_job_marker() {
        assert_eq!(
            find_staging_root("/d/files/.staging/job1/a/x.md", "job1").as_deref(),
            Some("/d/files/.staging/job1")
        );
        assert_eq!(find_staging_root("/d/files/final/a/x.md", "job1"), None);
    }

    #[test]
    fn finalize_success_then_cleanup() {
        let mut host = OkHost::default();
        let writes = vec![
            finalize("/d/.staging/job1/a/x.md", "/d/store/a/x.md"),
            write("chats.update"),
        ];
        apply_writes(&mut host, "job1", &writes, Some("EMBEDDING_GENERATE")).unwrap();
        // ensureDir(dirname(final)) then rename(staging -> final).
        assert_eq!(host.mkdirs, vec!["/d/store/a"]);
        assert_eq!(
            host.renames,
            vec![(
                "/d/.staging/job1/a/x.md".to_string(),
                "/d/store/a/x.md".to_string()
            )]
        );
        // __finalizeFile is intercepted, never dispatched to a repo.
        assert_eq!(host.dispatched, vec!["chats.update"]);
        // Post-commit cleanup of the derived staging root.
        assert_eq!(host.cleaned, vec!["/d/.staging/job1"]);
    }

    #[test]
    fn rollback_undoes_finalizes_in_reverse() {
        let mut host = OkHost {
            fail_on: Some("chats.update".to_string()),
            ..Default::default()
        };
        let writes = vec![
            finalize("/d/.staging/j/s1", "/d/store/f1"),
            finalize("/d/.staging/j/s2", "/d/store/f2"),
            write("chats.update"), // fails -> ROLLBACK + undo
        ];
        let err = apply_writes(&mut host, "j", &writes, Some("EMBEDDING_GENERATE")).unwrap_err();
        assert_eq!(err.message, "boom");
        // Forward renames, then undo in reverse (f2->s2, then f1->s1).
        assert_eq!(
            host.renames,
            vec![
                ("/d/.staging/j/s1".to_string(), "/d/store/f1".to_string()),
                ("/d/.staging/j/s2".to_string(), "/d/store/f2".to_string()),
                ("/d/store/f2".to_string(), "/d/.staging/j/s2".to_string()),
                ("/d/store/f1".to_string(), "/d/.staging/j/s1".to_string()),
            ]
        );
        // Throw short-circuits before post-commit cleanup/invalidation.
        assert!(host.cleaned.is_empty());
        assert!(host.invalidations.is_empty());
        // Main partition rolled back.
        assert!(host
            .exec
            .contains(&(WriteDbTarget::Main, "ROLLBACK".to_string())));
    }

    #[test]
    fn invalidations_dedup_in_first_seen_order() {
        let mut host = OkHost::default();
        let mk = |method: &str, id_field: &str, id: &str| ChildWritePayload {
            method: method.to_string(),
            args: vec![json!({ id_field: id })],
        };
        let writes = vec![
            mk("memories.create", "characterId", "charX"),
            mk("memories.delete", "characterId", "charX"), // dup char -> collapsed
            mk("vectorIndices.saveMeta", "characterId", "charY"),
            mk("docMountChunks.upsert", "mountPointId", "MP"),
            mk("chats.update", "characterId", "charZ"), // not an invalidating method
        ];
        apply_writes(&mut host, "j", &writes, Some("EMBEDDING_GENERATE")).unwrap();
        assert_eq!(
            host.invalidations,
            vec![
                ("vectorStore".to_string(), "charX".to_string()),
                ("vectorStore".to_string(), "charY".to_string()),
                ("mountPoint".to_string(), "MP".to_string()),
            ]
        );
    }
}
