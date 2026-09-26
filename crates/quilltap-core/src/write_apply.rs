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
//! - `runRefusalLedgerChecks` (P4.D225, v4 `49059fb14`) — post-commit, the
//!   Concierge's auto-switch check once per chat whose refusal ledger the
//!   batch incremented ([`chats_with_recorded_refusals`]); the host runs the
//!   check (production: `refusal_ledger::maybe_auto_switch_after_refusal`).
//!   Like the realtime hook, dormant until a handler batches.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::services::dangerous_content::refusal_ledger::LastRefusal;
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

    /// Post-commit: the Concierge's auto-switch check for ONE chat whose
    /// refusal ledger the batch incremented (v4 `maybeAutoSwitchAfterRefusal(
    /// chatId, lastRefusal)` — production wires
    /// [`crate::services::dangerous_content::refusal_ledger::maybe_auto_switch_after_refusal`]).
    /// An `Err` is v4's throw out of the loop: the engine logs it and checks
    /// no further chat, and the committed batch still resolves.
    fn run_refusal_ledger_check(
        &mut self,
        chat_id: &str,
        last_refusal: Option<&LastRefusal>,
    ) -> Result<(), ApplyError>;

    /// Post-commit: the classifier switch for ONE chat the batch classified
    /// dangerous (v4 `maybeSwitchAfterClassification(chatId, verdict)`,
    /// `4d370a90f` — production wires
    /// [`crate::services::dangerous_content::classifier_switch::maybe_switch_after_classification`]).
    /// `verdict` is the write's third argument when it is an object. An `Err`
    /// is v4's throw out of the loop: the engine logs it and switches no
    /// further chat, and the committed batch still resolves. Required, not
    /// defaulted, so no implementor can silently drop it.
    fn run_classifier_switch_check(
        &mut self,
        chat_id: &str,
        verdict: Option<&Value>,
    ) -> Result<(), ApplyError>;
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

    // The Concierge's refusal ledger (v4 `49059fb14`): a child can buffer an
    // increment but can neither read the resulting count nor act on it, so the
    // auto-switch check runs here, where the count is authoritative — after
    // every partition committed (a partition throw returned above), and still
    // inside the apply chain, so the flip's own writes cannot land in another
    // job's open transaction.
    run_refusal_ledger_checks(host, writes, job_id);
    // The classifier's switch (v4 `4d370a90f`): the job records telemetry only;
    // whether a dangerous verdict moves the chat is decided here, after the
    // batch committed, against the chat as it stands now. After the ledger
    // checks, as v4 orders them.
    run_classifier_switch_checks(host, writes, job_id);
    Ok(())
}

/// v4 `DANGER_CLASSIFICATION_WRITE` — the buffered write that records the
/// chat-level danger classifier's verdict (`4d370a90f`).
pub const DANGER_CLASSIFICATION_WRITE: &str = "chats.setDangerClassification";

/// v4 `chatsWithDangerVerdicts` — every chat this batch classified DANGEROUS
/// (`telemetry.isDangerousChat === true`), once each in first-write order (a JS
/// `Map`'s `set` on an existing key keeps its place), with the verdict the
/// write carried — its optional third argument when it is an object, else
/// `None`; the LAST one wins. A later SAFE write for the same chat does not
/// remove it (v4 `continue`s past it). A `chatId` that is not a non-empty
/// string is skipped.
pub fn chats_with_danger_verdicts(writes: &[ChildWritePayload]) -> Vec<(String, Option<Value>)> {
    let mut out: Vec<(String, Option<Value>)> = Vec::new();
    for w in writes {
        if w.method != DANGER_CLASSIFICATION_WRITE {
            continue;
        }
        let Some(chat_id) = w
            .args
            .first()
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let dangerous = w
            .args
            .get(1)
            .and_then(Value::as_object)
            .and_then(|t| t.get("isDangerousChat"))
            == Some(&Value::Bool(true));
        if !dangerous {
            continue;
        }
        let verdict = w.args.get(2).filter(|v| v.is_object()).cloned();
        match out.iter_mut().find(|(id, _)| id == chat_id) {
            Some((_, previous)) => *previous = verdict,
            None => out.push((chat_id.to_string(), verdict)),
        }
    }
    out
}

/// v4 `runClassifierSwitchChecks` — the classifier switch once per chat a
/// committed batch classified dangerous. Best-effort like the refusal-ledger
/// check: the batch is committed, so a failure is logged and never fails the
/// job. A replayed batch cannot switch twice: the switch's own Moderated-only
/// rule makes the second a no-op.
fn run_classifier_switch_checks(
    host: &mut dyn ApplyHost,
    writes: &[ChildWritePayload],
    job_id: &str,
) {
    let chats = chats_with_danger_verdicts(writes);
    if chats.is_empty() {
        return;
    }
    // v4 logs the id ARRAY; the file layer's `…Json` convention re-parses it.
    let chat_ids = Value::Array(
        chats
            .iter()
            .map(|(id, _)| Value::String(id.clone()))
            .collect(),
    )
    .to_string();
    tracing::debug!(
        target: "quilltap::jobs_dispatcher",
        job_id,
        chatIdsJson = chat_ids.as_str(),
        "Child batch recorded a dangerous classification; running the classifier switch"
    );
    for (chat_id, verdict) in &chats {
        if let Err(e) = host.run_classifier_switch_check(chat_id, verdict.as_ref()) {
            tracing::error!(
                target: "quilltap::jobs_dispatcher",
                job_id,
                error = %e,
                "Classifier switch failed after a committed batch"
            );
            return;
        }
    }
}

/// v4 `REFUSAL_LEDGER_INCREMENT` — the buffered write that records a
/// moderation refusal on a chat.
pub const REFUSAL_LEDGER_INCREMENT: &str = "chats.incrementModerationRefusalCount";

/// v4 `chatsWithRecordedRefusals` — every chat whose refusal ledger this batch
/// incremented, once each in first-increment order (a JS `Map`'s `set` on an
/// existing key keeps its place), with the last refusing provider the batch
/// named for it (the increment's optional third argument).
///
/// The rule is `who ?? previous ?? null`: v4's comment says "later increments
/// overwrite earlier ones", but a later increment WITHOUT a provider does not
/// erase an earlier one's — only a later NAMED provider replaces it. A
/// `chatId` that is not a non-empty string is skipped; a `refusedBy` counts
/// only when it is an object with a string `provider`.
pub fn chats_with_recorded_refusals(
    writes: &[ChildWritePayload],
) -> Vec<(String, Option<LastRefusal>)> {
    let mut out: Vec<(String, Option<LastRefusal>)> = Vec::new();
    for w in writes {
        if w.method != REFUSAL_LEDGER_INCREMENT {
            continue;
        }
        let Some(chat_id) = w
            .args
            .first()
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let who = w.args.get(2).and_then(|r| {
            let obj = r.as_object()?;
            let provider = obj.get("provider")?.as_str()?;
            Some(LastRefusal {
                provider: provider.to_string(),
                model_name: obj
                    .get("modelName")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        });
        match out.iter_mut().find(|(id, _)| id == chat_id) {
            Some((_, previous)) => {
                if who.is_some() {
                    *previous = who;
                }
            }
            None => out.push((chat_id.to_string(), who)),
        }
    }
    out
}

/// v4 `runRefusalLedgerChecks` — the auto-switch check once per chat whose
/// ledger a committed batch changed. Best-effort: the writes are committed, so
/// a failure is logged and never fails the job.
fn run_refusal_ledger_checks(host: &mut dyn ApplyHost, writes: &[ChildWritePayload], job_id: &str) {
    let chats = chats_with_recorded_refusals(writes);
    if chats.is_empty() {
        return;
    }
    // v4 logs the id ARRAY; the file layer's `…Json` convention re-parses it.
    let chat_ids = Value::Array(
        chats
            .iter()
            .map(|(id, _)| Value::String(id.clone()))
            .collect(),
    )
    .to_string();
    tracing::debug!(
        target: "quilltap::jobs_dispatcher",
        job_id,
        chatIdsJson = chat_ids.as_str(),
        "Child batch recorded moderation refusals; running the auto-switch check"
    );
    for (chat_id, last_refusal) in &chats {
        if let Err(e) = host.run_refusal_ledger_check(chat_id, last_refusal.as_ref()) {
            tracing::error!(
                target: "quilltap::jobs_dispatcher",
                job_id,
                error = %e,
                "Refusal-ledger auto-switch check failed after a committed batch"
            );
            return;
        }
    }
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
    // Realtime hints for whatever entities this batch touched (v4
    // `job-dispatcher.ts:525`, `f3892158d`). Separate from the cache
    // invalidation below — that one is about THIS process's in-memory caches,
    // this one about every open client's query cache — but they belong at the
    // same moment: the writes have committed and are readable. Published
    // BEFORE the early return below, as v4 publishes before its own collection
    // pass: a batch with no vector/mount keys can still have moved a chat.
    for hint in crate::realtime::job_topics::topics_for_write_batch(writes) {
        crate::realtime::bus::publish_realtime(hint.topic, hint.id.as_deref());
    }

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
        ledger_checks: Vec<(String, Option<LastRefusal>)>,
        ledger_check_fails: bool,
        switch_checks: Vec<(String, Option<Value>)>,
        switch_check_fails: bool,
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
        fn run_refusal_ledger_check(
            &mut self,
            chat_id: &str,
            last_refusal: Option<&LastRefusal>,
        ) -> Result<(), ApplyError> {
            self.ledger_checks
                .push((chat_id.to_string(), last_refusal.cloned()));
            if self.ledger_check_fails {
                return Err(ApplyError::msg("flip failed"));
            }
            Ok(())
        }
        fn run_classifier_switch_check(
            &mut self,
            chat_id: &str,
            verdict: Option<&Value>,
        ) -> Result<(), ApplyError> {
            self.switch_checks
                .push((chat_id.to_string(), verdict.cloned()));
            if self.switch_check_fails {
                return Err(ApplyError::msg("switch failed"));
            }
            Ok(())
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

    // ── P4.D124: the post-commit realtime hook ───────────────────────────
    //
    // `dispatch_invalidations` is v4's `dispatchInvalidations` twin, and
    // `f3892158d` put the realtime publish at the TOP of it — before its own
    // collection pass, so a batch that moves no vector-store or mount-point
    // cache key can still announce the chat it touched. v5's version early-
    // returns when both key lists are empty, which is exactly why the publish
    // has to sit above it.
    //
    // ⚠ **Dormant in production today, and that is recorded not hidden.** v5's
    // job handlers write directly rather than buffering (see
    // `services::job_runner`'s header: no W4.8 handler currently batches), so
    // `apply_writes` runs only from the batch-mode path the autonomous turn
    // reserves. The hook is wired 1:1 with v4 anyway, and driven here
    // synthetically, so it is correct the day a handler batches.

    #[tokio::test]
    async fn a_committed_batch_publishes_its_entity_hints() {
        let mut cap = crate::realtime::publish_sites::HintCapture::start();
        let mut host = OkHost::default();
        let writes = vec![
            ChildWritePayload {
                method: "chats.update".into(),
                args: vec![json!("c-1"), json!({ "title": "x" })],
            },
            ChildWritePayload {
                method: "characters.update".into(),
                args: vec![json!("ch-1")],
            },
        ];
        apply_writes(&mut host, "j", &writes, Some("EMBEDDING_GENERATE")).unwrap();

        assert_eq!(
            cap.drain_sorted_expecting(2).await,
            vec![
                ("characters".to_string(), Some("ch-1".to_string())),
                ("chats".to_string(), Some("c-1".to_string())),
            ]
        );
    }

    /// A batch whose writes move no CACHE key still publishes — the arm v5's
    /// early return would swallow if the publish sat below it.
    #[tokio::test]
    async fn a_batch_with_no_cache_keys_still_publishes() {
        let mut cap = crate::realtime::publish_sites::HintCapture::start();
        let mut host = OkHost::default();
        let writes = vec![ChildWritePayload {
            method: "chats.update".into(),
            args: vec![json!("c-1")],
        }];
        apply_writes(&mut host, "j", &writes, Some("EMBEDDING_GENERATE")).unwrap();
        assert!(
            host.invalidations.is_empty(),
            "the premise: this batch moves no cache key"
        );
        assert_eq!(
            cap.drain_expecting(1).await,
            vec![("chats".to_string(), Some("c-1".to_string()))]
        );
    }

    /// A FAILED batch publishes nothing: `apply_writes` short-circuits before
    /// the post-commit side effects, exactly as v4 does.
    #[tokio::test]
    async fn a_failed_batch_publishes_nothing() {
        let mut cap = crate::realtime::publish_sites::HintCapture::start();
        let mut host = OkHost {
            fail_on: Some("chats.update".into()),
            ..Default::default()
        };
        let writes = vec![ChildWritePayload {
            method: "chats.update".into(),
            args: vec![json!("c-1")],
        }];
        assert!(apply_writes(&mut host, "j", &writes, Some("EMBEDDING_GENERATE")).is_err());
        assert_eq!(cap.drain().await, vec![]);
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

    // ── P4.D225: the refusal-ledger post-commit hook ─────────────────────
    //
    // Dormant in production for the same reason as the realtime hook above
    // (no handler batches today); wired 1:1 with v4 and driven synthetically.

    fn increment(chat: Value, refused_by: Option<Value>) -> ChildWritePayload {
        let mut args = vec![chat, json!("2026-09-25T00:00:00Z")];
        if let Some(r) = refused_by {
            args.push(r);
        }
        ChildWritePayload {
            method: REFUSAL_LEDGER_INCREMENT.into(),
            args,
        }
    }

    fn last(provider: &str, model: Option<&str>) -> Option<LastRefusal> {
        Some(LastRefusal {
            provider: provider.into(),
            model_name: model.map(str::to_string),
        })
    }

    /// v4's own dispatcher-apply case, plus the `?? previous` half its comment
    /// misdescribes (M8): a later provider-less increment keeps the earlier
    /// provider; a later NAMED one replaces it; order is first-increment.
    #[test]
    fn chats_with_recorded_refusals_keeps_the_last_named_provider() {
        let writes = vec![
            increment(
                json!("c1"),
                Some(json!({ "provider": "GOOGLE", "modelName": "a" })),
            ),
            ChildWritePayload {
                method: "chats.update".into(),
                args: vec![json!("c1"), json!({ "title": "x" })],
            },
            increment(
                json!("c1"),
                Some(json!({ "provider": "OPENAI", "modelName": "b" })),
            ),
            increment(json!("c2"), None),
            increment(json!("c3"), Some(json!({ "provider": "XAI" }))),
            increment(json!("c3"), None),
            increment(json!(""), Some(json!({ "provider": "X" }))),
            increment(json!(7), Some(json!({ "provider": "X" }))),
            increment(json!("c4"), Some(json!({ "provider": 3 }))),
            increment(json!("c4"), Some(json!(["GOOGLE"]))),
        ];
        assert_eq!(
            chats_with_recorded_refusals(&writes),
            vec![
                ("c1".to_string(), last("OPENAI", Some("b"))),
                ("c2".to_string(), None),
                ("c3".to_string(), last("XAI", None)),
                ("c4".to_string(), None),
            ]
        );
    }

    #[test]
    fn the_check_runs_once_per_chat_after_every_partition_committed() {
        let mut host = OkHost::default();
        let writes = vec![
            increment(json!("c1"), Some(json!({ "provider": "GOOGLE" }))),
            increment(json!("c1"), None),
            increment(json!("c2"), None),
        ];
        let ((), lines) = crate::test_support::captured_with(|| {
            apply_writes(
                &mut host,
                "job-ledger",
                &writes,
                Some("STORY_BACKGROUND_GENERATION"),
            )
            .unwrap()
        });
        assert_eq!(
            host.ledger_checks,
            vec![
                ("c1".to_string(), last("GOOGLE", None)),
                ("c2".to_string(), None),
            ]
        );
        assert_eq!(
            host.exec.last().map(|(_, sql)| sql.as_str()),
            Some("COMMIT"),
            "the checks ran after the commit"
        );
        let ours: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("quilltap::jobs_dispatcher"))
            .collect();
        assert_eq!(
            ours,
            vec![
                "DEBUG quilltap::jobs_dispatcher Child batch recorded moderation refusals; running the auto-switch check job_id=job-ledger chatIdsJson=[\"c1\",\"c2\"]"
            ]
        );
    }

    #[test]
    fn no_refusal_in_the_batch_runs_no_check_and_logs_nothing() {
        let mut host = OkHost::default();
        let ((), lines) = crate::test_support::captured_with(|| {
            apply_writes(
                &mut host,
                "j",
                &[write("chats.update")],
                Some("TITLE_UPDATE"),
            )
            .unwrap()
        });
        assert!(host.ledger_checks.is_empty());
        assert!(!lines
            .iter()
            .any(|l| l.contains("quilltap::jobs_dispatcher")));
    }

    #[test]
    fn a_failed_partition_runs_no_check() {
        let mut host = OkHost {
            fail_on: Some(REFUSAL_LEDGER_INCREMENT.into()),
            ..OkHost::default()
        };
        let writes = vec![increment(json!("c1"), None)];
        assert!(
            apply_writes(&mut host, "j", &writes, Some("STORY_BACKGROUND_GENERATION")).is_err()
        );
        assert!(host.ledger_checks.is_empty());
    }

    /// v4: a throw inside the loop is caught once — logged, no further chat
    /// checked, the committed job still resolves.
    #[test]
    fn a_failing_check_never_fails_the_committed_job() {
        let mut host = OkHost {
            ledger_check_fails: true,
            ..OkHost::default()
        };
        let writes = vec![increment(json!("c1"), None), increment(json!("c2"), None)];
        let (result, lines) = crate::test_support::captured_with(|| {
            apply_writes(
                &mut host,
                "job-flip-fails",
                &writes,
                Some("STORY_BACKGROUND_GENERATION"),
            )
        });
        assert!(result.is_ok());
        assert_eq!(host.ledger_checks.len(), 1, "the loop stops at the throw");
        assert!(lines.iter().any(|l| l
            == "ERROR quilltap::jobs_dispatcher Refusal-ledger auto-switch check failed after a committed batch job_id=job-flip-fails error=flip failed"));
    }

    // ---- P4.D226 (v4 `4d370a90f`): the classifier-switch commit hook ----

    fn classification(
        chat_id: Value,
        dangerous: bool,
        verdict: Option<Value>,
    ) -> ChildWritePayload {
        let mut args = vec![
            chat_id,
            json!({ "isDangerousChat": dangerous, "dangerScore": 0.9 }),
        ];
        if let Some(v) = verdict {
            args.push(v);
        }
        ChildWritePayload {
            method: DANGER_CLASSIFICATION_WRITE.into(),
            args,
        }
    }

    /// v4 `chatsWithDangerVerdicts`: dangerous only, first-write order, the
    /// LAST verdict wins, a non-object verdict reads `None`, a later safe write
    /// never removes a chat, a non-string or empty id is skipped.
    #[test]
    fn the_scan_keeps_first_order_and_the_last_verdict() {
        let v1 = json!({ "score": 0.91 });
        let v2 = json!({ "score": 0.95 });
        let writes = vec![
            classification(json!("c1"), true, Some(v1.clone())),
            classification(json!("c2"), false, None),
            classification(json!("c3"), true, None),
            classification(json!("c1"), true, Some(v2.clone())),
            classification(json!("c2"), true, Some(json!("not-an-object"))),
            classification(json!(""), true, Some(v1.clone())),
            classification(json!(42), true, Some(v1.clone())),
            classification(json!("c3"), false, Some(v1)),
        ];
        assert_eq!(
            chats_with_danger_verdicts(&writes),
            vec![
                ("c1".to_string(), Some(v2)),
                ("c3".to_string(), None),
                ("c2".to_string(), None),
            ]
        );
    }

    /// After every partition committed and after the ledger hook; the DEBUG
    /// names the chats as the `…Json` array; silence on a batch with no
    /// dangerous classification.
    #[test]
    fn the_switch_runs_after_commit_and_after_the_ledger_hook() {
        let mut host = OkHost::default();
        let writes = vec![
            classification(json!("c1"), true, Some(json!({ "score": 0.9 }))),
            increment(json!("c2"), None),
        ];
        let (_, lines) = crate::test_support::captured_with(|| {
            apply_writes(
                &mut host,
                "job-switch",
                &writes,
                Some("CHAT_DANGER_CLASSIFICATION"),
            )
            .unwrap()
        });
        assert_eq!(host.switch_checks.len(), 1);
        let dispatcher: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains("quilltap::jobs_dispatcher"))
            .collect();
        assert_eq!(dispatcher, [
            "DEBUG quilltap::jobs_dispatcher Child batch recorded moderation refusals; running the auto-switch check job_id=job-switch chatIdsJson=[\"c2\"]",
            "DEBUG quilltap::jobs_dispatcher Child batch recorded a dangerous classification; running the classifier switch job_id=job-switch chatIdsJson=[\"c1\"]",
        ]);

        let mut quiet = OkHost::default();
        let safe = vec![classification(json!("c1"), false, None)];
        let (_, lines) = crate::test_support::captured_with(|| {
            apply_writes(&mut quiet, "j", &safe, Some("CHAT_DANGER_CLASSIFICATION")).unwrap()
        });
        assert!(quiet.switch_checks.is_empty());
        assert!(!lines.iter().any(|l| l.contains("classifier switch")));
    }

    #[test]
    fn a_failed_partition_runs_no_switch() {
        let mut host = OkHost {
            fail_on: Some(DANGER_CLASSIFICATION_WRITE.into()),
            ..OkHost::default()
        };
        let writes = vec![classification(json!("c1"), true, None)];
        assert!(apply_writes(&mut host, "j", &writes, Some("CHAT_DANGER_CLASSIFICATION")).is_err());
        assert!(host.switch_checks.is_empty());
    }

    /// v4: a failing switch is caught once — logged, no further chat, the
    /// committed job still resolves.
    #[test]
    fn a_failing_switch_never_fails_the_committed_job() {
        let mut host = OkHost {
            switch_check_fails: true,
            ..OkHost::default()
        };
        let writes = vec![
            classification(json!("c1"), true, None),
            classification(json!("c2"), true, None),
        ];
        let (result, lines) = crate::test_support::captured_with(|| {
            apply_writes(
                &mut host,
                "job-switch-fails",
                &writes,
                Some("CHAT_DANGER_CLASSIFICATION"),
            )
        });
        assert!(result.is_ok());
        assert_eq!(host.switch_checks.len(), 1, "the loop stops at the throw");
        assert!(lines.iter().any(|l| l
            == "ERROR quilltap::jobs_dispatcher Classifier switch failed after a committed batch job_id=job-switch-fails error=switch failed"));
    }
}
