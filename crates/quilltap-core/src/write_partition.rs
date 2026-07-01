//! Port of lib/background-jobs/host/write-partition.ts — the parent-side
//! write-batch classification + partition logic and the folder-conflict id
//! remap.
//!
//! A background-job handler buffers ALL of its repository writes into one batch
//! the writer applies. Those writes can target three SEPARATE databases — the
//! **main** DB, the dedicated **mount-index** DB, and the **llm-logs** DB. The
//! applier commits each partition in its OWN transaction on its OWN connection,
//! so a failure in one database can neither roll back nor leak into another.
//! This module holds the pure (no-I/O, fully unit-testable) classification +
//! partition logic — exactly the architectural invariants CLAUDE.md says to keep
//! from v4 (per-database partitioned apply, main-primary vs idempotent ordering,
//! the folder-conflict id remap).
//!
//! In v4 this is parent-process machinery; in the native core the "single writer"
//! becomes an ownership rule (one writer task holds the RW connection), but this
//! partition/remap logic ports directly — it is a correctness property, not a
//! Node workaround.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which dedicated SQLite database a buffered write targets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WriteDbTarget {
    Main,
    MountIndex,
    LlmLogs,
}

impl WriteDbTarget {
    /// Canonical wire string (matches the TS union member names).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::MountIndex => "mountIndex",
            Self::LlmLogs => "llmLogs",
        }
    }
}

/// Repository keys whose rows live in the dedicated mount-index database. Mirrors
/// the repos that override `getCollection()` to use the mount-index DB. Keep in
/// sync when adding a repo backed by the mount-index DB.
pub const MOUNT_INDEX_REPO_KEYS: &[&str] = &[
    "docMountPoints",
    "docMountFiles",
    "docMountFileLinks",
    "docMountFolders",
    "docMountChunks",
    "docMountDocuments",
    "docMountBlobs",
    "projectDocMountLinks",
];

/// Repository keys whose rows live in the dedicated llm-logs database.
pub const LLM_LOGS_REPO_KEYS: &[&str] = &["llmLogs"];

/// The dotted method that creates a `doc_mount_folders` row.
pub const DOC_MOUNT_FOLDER_CREATE: &str = "docMountFolders.create";

/// The built-in (`__`-prefixed) write that stages a file rename into place inside
/// the main-DB transaction. Not a repo dispatch — the applier intercepts it.
pub const FINALIZE_FILE: &str = "__finalizeFile";

/// Fields on a mount-index write's data object (`args[0]`) that hold a folder id
/// and therefore may need rewriting when an earlier same-batch folder create was
/// reconciled to an already-existing folder row. `parentId` lives on folder
/// rows; `folderId` lives on file-link rows.
pub const FOLDER_REF_FIELDS: &[&str] = &["parentId", "folderId"];

/// Job types whose **main-DB** writes must survive a failure in a *secondary*
/// database's writes. These handlers are NOT idempotent under retry — re-running
/// one to recover a dropped secondary write would duplicate the chat turn — so
/// the applier commits the main partition first and authoritatively, then applies
/// secondary partitions best-effort. Every other job type is idempotent and uses
/// all-or-nothing semantics. (Decision recorded 2026-06-05.)
pub const MAIN_PRIMARY_JOB_TYPES: &[&str] = &["AUTONOMOUS_ROOM_TURN"];

/// A single buffered write: a dotted repo method (or a `__`-prefixed built-in)
/// plus its positional args (JSON-serialisable, since they cross IPC in v4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildWritePayload {
    pub method: String,
    pub args: Vec<Value>,
}

/// Classify which database a single buffered write targets.
///
/// `__finalizeFile` is a built-in filesystem rename performed inside the main-DB
/// transaction, so it rides with the `Main` partition. Anything whose repo key
/// isn't explicitly mount-index or llm-logs defaults to `Main` — the safe
/// default, since main is the all-or-nothing primary partition.
pub fn classify_write_target(method: &str) -> WriteDbTarget {
    if method == FINALIZE_FILE {
        return WriteDbTarget::Main;
    }
    // `method.split('.', 1)[0]` in TS — the segment before the first dot (the
    // whole string when there is no dot).
    let repo_key = method.split('.').next().unwrap_or(method);
    if MOUNT_INDEX_REPO_KEYS.contains(&repo_key) {
        WriteDbTarget::MountIndex
    } else if LLM_LOGS_REPO_KEYS.contains(&repo_key) {
        WriteDbTarget::LlmLogs
    } else {
        WriteDbTarget::Main
    }
}

/// A batch split by target database, preserving per-partition write order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PartitionedWrites {
    pub main: Vec<ChildWritePayload>,
    pub mount_index: Vec<ChildWritePayload>,
    pub llm_logs: Vec<ChildWritePayload>,
}

/// Split a write batch into per-database partitions, preserving the original
/// relative order within each partition (intra-partition ordering carries
/// dependencies — e.g. a folder must be created before the file that lives in it).
pub fn partition_writes(writes: &[ChildWritePayload]) -> PartitionedWrites {
    let mut out = PartitionedWrites::default();
    for w in writes {
        match classify_write_target(&w.method) {
            WriteDbTarget::Main => out.main.push(w.clone()),
            WriteDbTarget::MountIndex => out.mount_index.push(w.clone()),
            WriteDbTarget::LlmLogs => out.llm_logs.push(w.clone()),
        }
    }
    out
}

/// True when a job's main-DB writes take priority over secondary-DB writes.
/// `None` (the TS `undefined`) is never main-primary.
pub fn is_main_primary_job_type(job_type: Option<&str>) -> bool {
    match job_type {
        Some(j) => MAIN_PRIMARY_JOB_TYPES.contains(&j),
        None => false,
    }
}

/// Rewrite folder-id references in a write's data object (`args[0]`) using a
/// remap of `bufferedFolderId → existingFolderId`. Used by the mount-index
/// partition apply when a concurrent folder create was reconciled to an
/// already-existing row, so later writes in the same batch that point at the
/// (now-discarded) buffered folder id are redirected to the surviving row.
///
/// Non-mutating: returns an equal payload when nothing changed (an empty remap,
/// a missing/non-object `args[0]`, or no matching folder-ref field), or a copy
/// with a rewritten `args[0]` when it did. Only string-valued fields present in
/// the remap are rewritten; other args are preserved verbatim.
pub fn rewrite_folder_refs(
    write: &ChildWritePayload,
    remap: &HashMap<String, String>,
) -> ChildWritePayload {
    if remap.is_empty() {
        return write.clone();
    }
    let data = match write.args.first() {
        Some(Value::Object(map)) => map,
        // None (empty args), null, array, or any non-object → unchanged.
        _ => return write.clone(),
    };

    let mut next = data.clone();
    let mut changed = false;
    for field in FOLDER_REF_FIELDS {
        if let Some(Value::String(value)) = next.get(*field) {
            if let Some(mapped) = remap.get(value) {
                let mapped = mapped.clone();
                next.insert((*field).to_string(), Value::String(mapped));
                changed = true;
            }
        }
    }
    if !changed {
        return write.clone();
    }

    let mut args = write.args.clone();
    args[0] = Value::Object(next);
    ChildWritePayload {
        method: write.method.clone(),
        args,
    }
}

/// Whether an error is a SQLite uniqueness/primary-key constraint violation — the
/// signature of a concurrent folder create losing the race to an
/// already-committed row. Matches on the `SQLITE_CONSTRAINT*` error code first,
/// then falls back to a case-insensitive search of the message text.
///
/// Takes the error as a `serde_json::Value` to mirror the TS `unknown` parameter;
/// a non-object (including a JSON array, which carries no `code`/`message`) is
/// never a constraint error.
pub fn is_unique_constraint_error(err: &Value) -> bool {
    let obj = match err.as_object() {
        Some(o) => o,
        None => return false,
    };
    if let Some(Value::String(code)) = obj.get("code") {
        if code.starts_with("SQLITE_CONSTRAINT") {
            return true;
        }
    }
    if let Some(Value::String(message)) = obj.get("message") {
        return message.to_lowercase().contains("unique constraint failed");
    }
    false
}
