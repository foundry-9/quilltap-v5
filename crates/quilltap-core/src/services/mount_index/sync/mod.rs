//! `quilltap sync` — one run of the document-store sync.
//!
//! Port of v4 `lib/mount-index/sync/index.ts` (`23da0b322`) and its eight
//! sibling modules.
//!
//! Reads both sides once, plans, applies, writes the manifest, and reports. The
//! planner holds all the judgement; this module holds the plumbing: the
//! store-type guards, the per-store mutex that stops two runs from interleaving
//! their compare-and-swaps, the byte fetches each action needs, and the
//! bookkeeping that turns an applied plan into the next run's base.
//!
//! The verb never touches `doc_mount_chunks` or an embedding vector of its own
//! accord. It calls the same write chokepoints every other writer calls, so the
//! store's existing post-write hooks do the indexing — which is also why the
//! engine lives in the core rather than in the CLI: those hooks are the repo
//! layer, and a lock-gated direct-SQLite writer could not run them at all.
//!
//! ⚠ v4's own commit message for `23da0b322` says "chunks and vectors are never
//! read or written by the sync". That is false at module level and the port
//! follows the HUNKS: [`apply_store::write_store_file`] calls
//! `reindex_after_database_write` on the document branch and on pdf/docx, which
//! is exactly how the sync gets the store's re-chunk for free. What the sentence
//! means is that the sync issues no chunk SQL itself.

pub mod apply_disk;
pub mod apply_store;
pub mod manifest;
pub mod planner;
pub mod sidecar;
pub mod types;
pub mod walk_disk;
pub mod walk_store;

use std::collections::HashSet;
use std::fmt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;

use self::apply_disk::{
    apply_disk_action, birthtime_is_settable, ensure_target_directory, node_platform, node_resolve,
    resolve_in_target, target_exists, DiskApplyError,
};
use self::apply_store::{apply_store_action, read_store_bytes};
use self::manifest::{base_from_manifest, read_manifest, write_manifest, ReadManifestError};
use self::planner::{plan_sync, PlanInput};
use self::sidecar::description_sha256;
use self::types::{
    EntryKind, ManifestEntry, OrderedMap, SyncAction, SyncActionKind, SyncManifest, SyncOptions,
    SyncOutcome, SyncReport, SyncSide, SyncSummary, SYNC_MANIFEST_FILENAME,
};
use self::walk_disk::walk_disk;
use self::walk_store::walk_store;
use crate::clock::now_iso;
use crate::db::DbError;
use crate::services::mount_index::blob_transcode::WebpTranscoder;
use crate::services::mount_index::document_text_search::archived_character_vault_mount_point_ids;

/// The mount-point row the sync reads. v5's existing scoped rows each miss at
/// least one of these six columns, so the verb carries its own — assembled by
/// the API layer from the full DTO read, never by new SQL.
#[derive(Debug, Clone)]
pub struct SyncMountPoint {
    pub id: String,
    pub name: String,
    pub mount_type: String,
    pub base_path: String,
    pub store_type: String,
    pub scan_status: String,
    pub conversion_status: String,
    pub exclude_patterns: Vec<String>,
}

/// v4 `SyncRefusedError` — a refusal the operator can act on, carrying the code
/// the route maps to a status.
#[derive(Debug, Clone)]
pub struct SyncRefusedError {
    pub message: String,
    pub code: SyncRefusalCode,
}

impl fmt::Display for SyncRefusedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SyncRefusedError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRefusalCode {
    SyncInProgress,
    NotDatabaseBacked,
    ConversionInProgress,
    ScanInProgress,
    CharacterArchived,
}

impl SyncRefusalCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SyncInProgress => "SYNC_IN_PROGRESS",
            Self::NotDatabaseBacked => "NOT_DATABASE_BACKED",
            Self::ConversionInProgress => "CONVERSION_IN_PROGRESS",
            Self::ScanInProgress => "SCAN_IN_PROGRESS",
            Self::CharacterArchived => "CHARACTER_ARCHIVED",
        }
    }

    /// v4's route maps the three "busy" codes to 409 and the other two to 400.
    pub fn is_conflict(self) -> bool {
        matches!(
            self,
            Self::SyncInProgress | Self::ConversionInProgress | Self::ScanInProgress
        )
    }
}

/// Everything `sync_mount_point` can fail with before it has a report.
#[derive(Debug)]
pub enum SyncError {
    Refused(SyncRefusedError),
    ManifestMismatch(manifest::ManifestMismatchError),
    /// An `ENOENT` reaching the route's own "the path is resolved on the server"
    /// sentence, kept distinguishable from every other IO failure.
    NotFound(String),
    Io(String),
    Db(DbError),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(e) => e.fmt(f),
            Self::ManifestMismatch(e) => e.fmt(f),
            Self::NotFound(m) | Self::Io(m) => f.write_str(m),
            Self::Db(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<DbError> for SyncError {
    fn from(e: DbError) -> Self {
        Self::Db(e)
    }
}

/// v4's `const inFlight = new Set<string>()`.
///
/// One run per store at a time. A second run interleaved with the first would
/// see the first's half-applied writes as "changed since the plan" and report a
/// storm of conflicts — and on the disk side the two would race for the same
/// temp names.
fn in_flight() -> &'static Mutex<HashSet<String>> {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The one impure input the engine takes besides the connection: the host's
/// WebP encoder, handed to the blob writer so a synced image normalizes exactly
/// as any other store write does.
pub struct SyncDeps<'a> {
    pub transcoder: Option<&'a dyn WebpTranscoder>,
}

/// v4 `syncMountPoint(mountPoint, options)`.
pub fn sync_mount_point(
    conn: &Connection,
    main: &Connection,
    mount_point: &SyncMountPoint,
    options: &SyncOptions,
    deps: &SyncDeps<'_>,
) -> Result<SyncReport, SyncError> {
    let started = crate::clock::now_unix_ms();
    assert_syncable(main, mount_point)?;

    // v4 checks membership and inserts in two statements; a Mutex makes the pair
    // atomic, which is strictly safer and observably identical for one thread.
    {
        let mut set = in_flight().lock().expect("sync in-flight set");
        if set.contains(&mount_point.id) {
            return Err(SyncError::Refused(SyncRefusedError {
                message: format!("A sync of \"{}\" is already running", mount_point.name),
                code: SyncRefusalCode::SyncInProgress,
            }));
        }
        set.insert(mount_point.id.clone());
    }
    let result = run_sync(conn, mount_point, options, deps, started);
    in_flight()
        .lock()
        .expect("sync in-flight set")
        .remove(&mount_point.id);
    result
}

// ============================================================================
// Guards
// ============================================================================

fn assert_syncable(main: &Connection, mount_point: &SyncMountPoint) -> Result<(), SyncError> {
    let refuse = |message: String, code: SyncRefusalCode| {
        Err(SyncError::Refused(SyncRefusedError { message, code }))
    };

    if mount_point.mount_type != "database" {
        return refuse(
            format!(
                "\"{}\" is a {} store — it already IS a directory ({}). \
                 Sync mirrors database-backed stores only.",
                mount_point.name,
                mount_point.mount_type,
                // v4 `mountPoint.basePath || 'no base path recorded'` — JS
                // truthiness, so an empty string takes the fallback.
                if mount_point.base_path.is_empty() {
                    "no base path recorded"
                } else {
                    &mount_point.base_path
                },
            ),
            SyncRefusalCode::NotDatabaseBacked,
        );
    }
    if mount_point.conversion_status != "idle" {
        return refuse(
            format!(
                "\"{}\" is {}; wait for that to finish",
                mount_point.name, mount_point.conversion_status
            ),
            SyncRefusalCode::ConversionInProgress,
        );
    }
    if mount_point.scan_status == "scanning" {
        return refuse(
            format!(
                "\"{}\" is being scanned; wait for that to finish",
                mount_point.name
            ),
            SyncRefusalCode::ScanInProgress,
        );
    }
    if mount_point.store_type == "character" {
        // An archived character is a tombstone. Its vault is still a live,
        // writable store, so the guards are the only thing stopping a sync from
        // editing it back into existence.
        let archived = archived_character_vault_mount_point_ids(main)?;
        if archived.contains(&mount_point.id) {
            return refuse(
                format!(
                    "\"{}\" is an archived character's vault and cannot be synced. \
                     Rehydrate the character first.",
                    mount_point.name
                ),
                SyncRefusalCode::CharacterArchived,
            );
        }
    }
    Ok(())
}

// ============================================================================
// The run
// ============================================================================

fn run_sync(
    conn: &Connection,
    mount_point: &SyncMountPoint,
    options: &SyncOptions,
    deps: &SyncDeps<'_>,
    started: i64,
) -> Result<SyncReport, SyncError> {
    // v4 `path.resolve(options.targetPath)`. The CLI already sends an absolute
    // path (it expands `~` and resolves against its own cwd), so this only
    // normalizes; a caller that sent a relative one resolves against the
    // SERVER's cwd, which is v4's behaviour too.
    let target_path = node_resolve(&options.target_path);
    let mut warnings: Vec<String> = Vec::new();

    // `--dry-run` changes nothing on either side, and that includes not
    // conjuring the directory: an operator planning a sync against a path they
    // mistyped should be left with the mistyped path absent, not with an empty
    // folder they now have to notice and remove.
    if options.dry_run {
        if !target_exists(&target_path) {
            warnings.push(format!(
                "{} does not exist yet; a real run would create it",
                target_path.display()
            ));
        }
    } else {
        ensure_target_directory(&target_path).map_err(disk_error_to_sync)?;
    }

    let manifest = if options.use_manifest {
        match read_manifest(&target_path, &mount_point.id, &mut warnings) {
            Ok(m) => m,
            Err(ReadManifestError::Mismatch(e)) => return Err(SyncError::ManifestMismatch(e)),
            Err(ReadManifestError::Io(e)) => return Err(io_to_sync(&e, &target_path)),
        }
    } else {
        None
    };
    let base = base_from_manifest(manifest.as_ref());

    let store_walk = walk_store(conn, &mount_point.id)?;
    let disk_walk = walk_disk(&target_path, &mount_point.exclude_patterns);
    warnings.extend(store_walk.warnings.iter().cloned());
    warnings.extend(disk_walk.warnings.iter().cloned());

    // A store path that already uses the sidecar suffix is ambiguous in both
    // directions; it is dropped from the walk and named here instead.
    let reserved: Vec<SyncAction> = store_walk
        .reserved_paths
        .iter()
        .map(|relative_path| {
            let mut action =
                SyncAction::bare(SyncActionKind::Conflict, relative_path, EntryKind::File);
            action.reason =
                Some("reserved name: the sidecar suffix cannot also be a document".to_string());
            action.outcome = Some(SyncOutcome::Skipped);
            action
        })
        .collect();

    let planned = plan_sync(&PlanInput {
        store: &store_walk.entries,
        disk: &disk_walk.entries,
        base: &base,
        options,
        is_character_vault: mount_point.store_type == "character",
        can_set_disk_birthtime: birthtime_is_settable(),
    });
    warnings.extend(planned.warnings);

    let mut plan: Vec<SyncAction> = reserved;
    plan.extend(planned.actions);

    tracing::debug!(
        target: "quilltap::mount_index",
        mount_point_id = %mount_point.id,
        target_path = %target_path.display(),
        store_entries = store_walk.entries.len(),
        disk_entries = disk_walk.entries.len(),
        base_entries = base.len(),
        actions = plan.len(),
        dry_run = options.dry_run,
        "[Sync] Planned",
    );

    if !options.dry_run {
        apply_plan(
            conn,
            mount_point,
            &target_path,
            &mut plan,
            &mut warnings,
            deps,
        );
        if options.use_manifest {
            let built = build_manifest(conn, mount_point, &target_path)?;
            if let Err(e) = write_manifest(&target_path, &built) {
                return Err(io_to_sync(&e, &target_path));
            }
        }
        if !birthtime_is_settable()
            && plan.iter().any(|a| {
                a.side == Some(SyncSide::Disk) && a.created_at.as_ref().is_some_and(|c| c.is_some())
            })
        {
            warnings.push(format!(
                "Creation dates are not settable on {}; they are recorded in {SYNC_MANIFEST_FILENAME} instead",
                node_platform()
            ));
        }
    }

    let summary = summarise(&plan);
    Ok(SyncReport {
        store_id: mount_point.id.clone(),
        store_name: mount_point.name.clone(),
        target_path: target_path.to_string_lossy().into_owned(),
        dry_run: options.dry_run,
        actions: plan,
        summary,
        warnings,
        elapsed_ms: crate::clock::now_unix_ms() - started,
    })
}

/// Run the plan in order, recording each action's outcome on the action itself.
///
/// A failing action never aborts the run: the rest of the plan is independent of
/// it, and a sync that stopped at the first unwritable file would leave the
/// operator with a directory in a state no manifest describes. Failures are
/// reported and counted, and the CLI's exit code reflects them.
fn apply_plan(
    conn: &Connection,
    mount_point: &SyncMountPoint,
    target_path: &Path,
    plan: &mut [SyncAction],
    warnings: &mut Vec<String>,
    deps: &SyncDeps<'_>,
) {
    for action in plan.iter_mut() {
        let Some(side) = action.side else {
            continue;
        };
        let outcome = (|| -> Result<(), String> {
            let bytes = bytes_for(conn, mount_point, target_path, action)?;
            match side {
                SyncSide::Store => apply_store_action(
                    conn,
                    &mount_point.id,
                    action,
                    bytes.as_deref(),
                    deps.transcoder,
                )
                .map_err(|e| e.to_string()),
                SyncSide::Disk => apply_disk_action(target_path, action, bytes.as_deref())
                    .map_err(|e| e.to_string()),
            }
        })();

        match outcome {
            Ok(()) => {
                action.outcome = Some(SyncOutcome::Applied);
                tracing::debug!(
                    target: "quilltap::mount_index",
                    mount_point_id = %mount_point.id,
                    kind = %action.kind.as_str(),
                    side = %side.as_str(),
                    relative_path = %action.relative_path,
                    "[Sync] Applied",
                );
            }
            Err(message) => {
                action.outcome = Some(SyncOutcome::Failed);
                action.error = Some(message.clone());
                warnings.push(format!(
                    "{} {} {}: {message}",
                    action.kind.as_str(),
                    side.as_str(),
                    action.relative_path
                ));
                tracing::warn!(
                    target: "quilltap::mount_index",
                    mount_point_id = %mount_point.id,
                    kind = %action.kind.as_str(),
                    side = %side.as_str(),
                    relative_path = %action.relative_path,
                    error = %message,
                    "[Sync] Action failed",
                );
            }
        }
    }
}

/// The bytes an action needs, read from the side it is copying FROM. A hard-link
/// fan-out reads the store, which by then holds the written bytes — which is why
/// the planner orders store-side work first.
fn bytes_for(
    conn: &Connection,
    mount_point: &SyncMountPoint,
    target_path: &Path,
    action: &SyncAction,
) -> Result<Option<Vec<u8>>, String> {
    if !matches!(action.kind, SyncActionKind::Create | SyncActionKind::Modify) {
        return Ok(None);
    }

    if action.side == Some(SyncSide::Disk) {
        let bytes = read_store_bytes(conn, &mount_point.id, &action.relative_path)
            .map_err(|e| e.to_string())?;
        return match bytes {
            Some(b) => Ok(Some(b)),
            None => Err(format!(
                "The store has no content at {}",
                action.relative_path
            )),
        };
    }

    let absolute =
        resolve_in_target(target_path, &action.relative_path).map_err(|e| e.to_string())?;
    std::fs::read(&absolute)
        .map(Some)
        .map_err(|e| node_io_message(&e, "open", &absolute))
}

// ============================================================================
// Manifest
// ============================================================================

/// Re-read the store and re-stat the disk to record what the run actually left,
/// rather than what it meant to leave. A failed action must not be written into
/// the base as though it had succeeded — that is exactly the state that would
/// make the next run delete something.
fn build_manifest(
    conn: &Connection,
    mount_point: &SyncMountPoint,
    target_path: &Path,
) -> Result<SyncManifest, SyncError> {
    let store_walk = walk_store(conn, &mount_point.id)?;
    let disk_walk = walk_disk(target_path, &mount_point.exclude_patterns);

    let mut entries: OrderedMap<ManifestEntry> = OrderedMap::new();
    for (key, store) in store_walk.entries.iter() {
        let Some(disk) = disk_walk.entries.get(key) else {
            // Only what both sides agree on becomes the base. An entry present
            // on one side alone is genuinely "new there" next time, which is the
            // right answer.
            continue;
        };
        if store.kind() == EntryKind::Folder {
            entries.insert(
                store.relative_path.clone(),
                ManifestEntry {
                    kind: EntryKind::Folder,
                    created_at: Some(store.created_at.clone()),
                    ..ManifestEntry::default()
                },
            );
            continue;
        }
        if store.sha256 != disk.sha256 {
            continue;
        }
        // v4 spreads the two description keys in only when either side has a
        // non-empty one (JS truthiness on both), so an entry with no caption
        // carries neither key.
        let has_description = !store.description.as_deref().unwrap_or("").is_empty()
            || !disk.description.as_deref().unwrap_or("").is_empty();
        entries.insert(
            store.relative_path.clone(),
            ManifestEntry {
                kind: EntryKind::File,
                sha256: store.sha256.clone(),
                last_modified: Some(store.last_modified.clone()),
                created_at: Some(store.created_at.clone()),
                description_sha256: has_description
                    .then(|| description_sha256(store.description.as_deref().unwrap_or(""))),
                description_updated_at: has_description
                    .then(|| store.description_updated_at.clone()),
            },
        );
    }

    Ok(SyncManifest {
        version: 1,
        store_id: mount_point.id.clone(),
        store_name: mount_point.name.clone(),
        last_sync_at: now_iso(),
        entries,
    })
}

// ============================================================================
// Report
// ============================================================================

fn summarise(plan: &[SyncAction]) -> SyncSummary {
    let mut summary = SyncSummary::default();
    for action in plan {
        if action.outcome == Some(SyncOutcome::Failed) {
            summary.failed += 1;
            continue;
        }
        match action.kind {
            SyncActionKind::Create | SyncActionKind::Mkdir => summary.created += 1,
            SyncActionKind::Modify => summary.modified += 1,
            SyncActionKind::Delete | SyncActionKind::Rmdir => summary.deleted += 1,
            SyncActionKind::Touch => summary.touched += 1,
            SyncActionKind::Describe => summary.described += 1,
            SyncActionKind::Conflict => summary.conflicts += 1,
            SyncActionKind::Skip => summary.skipped += 1,
        }
    }
    summary
}

// ============================================================================
// Error plumbing
// ============================================================================

fn disk_error_to_sync(e: DiskApplyError) -> SyncError {
    match e {
        DiskApplyError::Io(io) => SyncError::Io(io.to_string()),
        other => SyncError::Io(other.to_string()),
    }
}

fn io_to_sync(e: &std::io::Error, path: &Path) -> SyncError {
    let message = node_io_message(e, "open", path);
    if e.kind() == std::io::ErrorKind::NotFound {
        SyncError::NotFound(message)
    } else {
        SyncError::Io(message)
    }
}

/// Node's `err.message` for an `fs` failure: `"<code>: <text>, <syscall>
/// '<path>'"`.
fn node_io_message(e: &std::io::Error, syscall: &str, path: &Path) -> String {
    let (code, text) = match e.kind() {
        std::io::ErrorKind::NotFound => ("ENOENT", "no such file or directory"),
        std::io::ErrorKind::PermissionDenied => ("EACCES", "permission denied"),
        std::io::ErrorKind::NotADirectory => ("ENOTDIR", "not a directory"),
        std::io::ErrorKind::IsADirectory => ("EISDIR", "illegal operation on a directory"),
        _ => ("EIO", "i/o error"),
    };
    format!("{code}: {text}, {syscall} '{}'", path.display())
}
