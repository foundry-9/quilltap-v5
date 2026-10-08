//! SQLite physical backups — port of v4 `lib/database/backends/sqlite/
//! physical-backup.ts` (P4.D259; never ported before this round).
//!
//! v4's header, carried: hot physical backups of each SQLite database —
//! byte-level copies independent of the logical (JSON) backup system. They
//! run automatically once per day (checked on startup, skipped if recent), are
//! stored under `<data>/backups/`, use `VACUUM INTO` (which keeps the
//! encryption key — the online-backup API would create an UNKEYED target), and
//! follow a retention policy: all for 7 days, weekly for 4 weeks, monthly for
//! 12 months, yearly forever.
//!
//! Two callers, both in `quilltap-host`, both v4's:
//!   - the PHASE 0.75 daily pass ([`super::daily_db_optimize`]) takes each
//!     database's backup BEFORE it optimizes it;
//!   - backend.ts `connect()`'s PHASE-2 startup backup ([`run_startup_backups`])
//!     runs the trio + retention after the partitions are up — on the day's
//!     first boot it finds the PHASE-0.75 files and skips (v4's own "the
//!     backend's own startup backup then finds it and skips").
//!
//! **Zones.** v4's filenames, parsers and Phase-4 year are the Node process's
//! LOCAL time (`getFullYear()` / `new Date(y, m-1, …)`); v5 takes the host's
//! display zone as a VALUE (`HostConfig::display_zone`, P4.140's Option V) and
//! the clock as `now_ms` — never UTC, never a second environment read.
//!
//! **Reachability, measured (the order assumed otherwise).** v4's 24-hour gate
//! (`shouldCreateBackup` → `readdirSync`) runs OUTSIDE each backup's `try`, so
//! a `data/backups` that exists but is not a directory makes every backup
//! function REJECT (`ENOTDIR: not a directory, scandir '…'`). That is the one
//! path that reaches the daily pass's `Pre-optimize backup threw` WARN and
//! backend.ts's three root ERRORs; [`create_physical_backup`]'s `Err` is that
//! rejection.
//!
//! Every `fs` failure renders Node's `err.message` (`<CODE>: <text>, <syscall>
//! '<path>'`) and every SQLite failure the bare SQLite message — v4 logs
//! `error.message` both ways.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::host_zone::TimeZone;

/// v4's child-logger module for this file.
pub const PHYSICAL_BACKUP_MODULE: &str = "database:physical-backup";

/// `getBackupsDir()` = `path.join(getDataDir(), 'backups')` (`lib/paths.ts:337`).
pub const BACKUPS_DIR_NAME: &str = "backups";

/// Minimum interval between automatic physical backups (24 hours).
pub const BACKUP_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// `<data>/backups`.
pub fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUPS_DIR_NAME)
}

/// Which database a backup is of. v4 keeps three parallel copies of every
/// function; the TEXTS differ per kind (not one template), so each is spelled
/// out here as v4 spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Main,
    LlmLogs,
    MountIndex,
}

impl BackupKind {
    /// The filename prefix before `YYYY-MM-DDTHHmmss.db`.
    pub fn prefix(self) -> &'static str {
        match self {
            BackupKind::Main => "quilltap-",
            BackupKind::LlmLogs => "quilltap-llm-logs-",
            BackupKind::MountIndex => "quilltap-mount-index-",
        }
    }

    /// The `label` `shouldCreateBackup` interpolates.
    fn gate_label(self) -> &'static str {
        match self {
            BackupKind::Main => "main database",
            BackupKind::LlmLogs => "LLM logs",
            BackupKind::MountIndex => "mount index",
        }
    }

    fn starting(self) -> &'static str {
        match self {
            BackupKind::Main => "Starting physical database backup",
            BackupKind::LlmLogs => "Starting LLM logs physical backup",
            BackupKind::MountIndex => "Starting mount index physical backup",
        }
    }

    fn created(self) -> &'static str {
        match self {
            BackupKind::Main => "Startup physical backup created",
            BackupKind::LlmLogs => "LLM logs physical backup created",
            BackupKind::MountIndex => "Mount index physical backup created",
        }
    }

    fn failed(self) -> &'static str {
        match self {
            BackupKind::Main => "Physical database backup failed",
            BackupKind::LlmLogs => "LLM logs physical backup failed",
            BackupKind::MountIndex => "Mount index physical backup failed",
        }
    }

    fn cleanup_failed(self) -> &'static str {
        match self {
            BackupKind::Main => "Failed to clean up partial backup file",
            BackupKind::LlmLogs => "Failed to clean up partial LLM logs backup file",
            BackupKind::MountIndex => "Failed to clean up partial mount index backup file",
        }
    }
}

/// A Unix-ms instant's civil parts in `zone` — what v4's `getFullYear()` /
/// `getMonth() + 1` / `getDate()` / `getHours()` / … answer in the process's
/// local zone.
pub(crate) fn local_parts(ms: i64, zone: &TimeZone) -> (i64, i8, i8, i8, i8, i8) {
    let ts = jiff::Timestamp::from_millisecond(ms).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    let z = ts.to_zoned(zone.clone());
    (
        i64::from(z.year()),
        z.month(),
        z.day(),
        z.hour(),
        z.minute(),
        z.second(),
    )
}

/// v4 `generate*BackupFilename` — `<prefix>YYYY-MM-DDTHHmmss.db`, LOCAL time.
pub fn generate_backup_filename(kind: BackupKind, now_ms: i64, zone: &TimeZone) -> String {
    let (y, mo, d, h, mi, s) = local_parts(now_ms, zone);
    format!("{}{y}-{mo:02}-{d:02}T{h:02}{mi:02}{s:02}.db", kind.prefix())
}

/// v4 `parse*BackupFilename` — the instant a backup filename names, or `None`.
/// The ONE shared parser (`almanack::phase1_premises::parse_backup_filename`,
/// the Almanack's reader of the same directory).
pub fn parse_backup_filename(kind: BackupKind, filename: &str, zone: &TimeZone) -> Option<i64> {
    crate::almanack::phase1_premises::parse_backup_filename(filename, kind.prefix(), zone)
}

/// Node's `err.message` for an `fs` failure: `<CODE>: <text>, <syscall>` plus
/// ` '<path>'` when the call names one (libuv's `uv_strerror` texts). Errnos
/// are matched by number where macOS and Linux agree, by `ErrorKind` where
/// they do not.
pub(crate) fn node_fs_message(e: &std::io::Error, syscall: &str, path: Option<&Path>) -> String {
    use std::io::ErrorKind as K;
    let by_errno = match e.raw_os_error() {
        Some(1) => Some(("EPERM", "operation not permitted")),
        Some(2) => Some(("ENOENT", "no such file or directory")),
        Some(5) => Some(("EIO", "i/o error")),
        Some(13) => Some(("EACCES", "permission denied")),
        Some(16) => Some(("EBUSY", "resource busy or locked")),
        Some(17) => Some(("EEXIST", "file already exists")),
        Some(18) => Some(("EXDEV", "cross-device link not permitted")),
        Some(20) => Some(("ENOTDIR", "not a directory")),
        Some(21) => Some(("EISDIR", "illegal operation on a directory")),
        Some(22) => Some(("EINVAL", "invalid argument")),
        Some(24) => Some(("EMFILE", "too many open files")),
        Some(28) => Some(("ENOSPC", "no space left on device")),
        Some(30) => Some(("EROFS", "read-only file system")),
        _ => None,
    };
    let (code, text) = by_errno.unwrap_or(match e.kind() {
        K::NotFound => ("ENOENT", "no such file or directory"),
        K::PermissionDenied => ("EACCES", "permission denied"),
        K::NotADirectory => ("ENOTDIR", "not a directory"),
        K::IsADirectory => ("EISDIR", "illegal operation on a directory"),
        K::AlreadyExists => ("EEXIST", "file already exists"),
        K::ReadOnlyFilesystem => ("EROFS", "read-only file system"),
        K::StorageFull => ("ENOSPC", "no space left on device"),
        _ => ("UNKNOWN", "unknown error"),
    });
    match path {
        Some(p) => format!("{code}: {text}, {syscall} '{}'", p.display()),
        None => format!("{code}: {text}, {syscall}"),
    }
}

/// Node's `fs.existsSync`: `true` iff `stat` succeeds (symlinks followed; any
/// error is `false`).
fn exists_sync(path: &Path) -> bool {
    std::fs::metadata(path).is_ok()
}

/// v4's `readdirSync` order: libuv's `scandir` sorts the names with `strcmp`,
/// i.e. bytewise.
fn readdir_sync(dir: &Path) -> Result<Vec<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| node_fs_message(&e, "scandir", Some(dir)))?;
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(names)
}

/// v4 `findMostRecentBackup` — the NEWEST parsed instant among `kind`'s files,
/// `None` when the directory is absent or holds none. A `readdirSync` failure
/// is v4's THROW (Node's message).
pub fn find_most_recent_backup(
    data_dir: &Path,
    kind: BackupKind,
    zone: &TimeZone,
) -> Result<Option<i64>, String> {
    let dir = backups_dir(data_dir);
    if !exists_sync(&dir) {
        return Ok(None);
    }
    let mut newest: Option<i64> = None;
    for filename in readdir_sync(&dir)? {
        if let Some(ms) = parse_backup_filename(kind, &filename, zone) {
            if newest.is_none_or(|n| ms > n) {
                newest = Some(ms);
            }
        }
    }
    Ok(newest)
}

/// JS `Math.round` (half toward +∞), with `-0` read as `0` (a JSON render
/// cannot tell them apart).
fn js_round(x: f64) -> f64 {
    let r = (x + 0.5).floor();
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

/// v4 `shouldCreateBackup` — whether enough time has passed since `kind`'s
/// newest backup, with its three DEBUGs. `Err` is the gate's `readdirSync`
/// throw, which v4 does NOT catch.
pub fn should_create_backup(
    data_dir: &Path,
    kind: BackupKind,
    now_ms: i64,
    zone: &TimeZone,
) -> Result<bool, String> {
    let label = kind.gate_label();
    let Some(last) = find_most_recent_backup(data_dir, kind, zone)? else {
        tracing::debug!(
            target: "quilltap::db",
            module = PHYSICAL_BACKUP_MODULE,
            "{}",
            format!("No existing {label} backups found, backup needed")
        );
        return Ok(true);
    };
    let age_ms = now_ms - last;
    let last_backup = crate::clock::iso_from_unix_ms(last);
    // `Math.round(ageMs / (60 * 60 * 1000) * 10) / 10` — a JS number: `25`,
    // never `25.0` (f64's Display prints a whole value bare).
    let age_hours = js_round(age_ms as f64 / (60.0 * 60.0 * 1000.0) * 10.0) / 10.0;
    if age_ms < BACKUP_INTERVAL_MS {
        tracing::debug!(
            target: "quilltap::db",
            module = PHYSICAL_BACKUP_MODULE,
            lastBackup = last_backup.as_str(),
            ageHours = age_hours,
            "{}",
            format!("Recent {label} backup exists, skipping")
        );
        return Ok(false);
    }
    tracing::debug!(
        target: "quilltap::db",
        module = PHYSICAL_BACKUP_MODULE,
        lastBackup = last_backup.as_str(),
        ageHours = age_hours,
        "{}",
        format!("Last {label} backup is old enough, backup needed")
    );
    Ok(true)
}

/// v4 `createPhysicalBackup` / `createLLMLogsPhysicalBackup` /
/// `createMountIndexPhysicalBackup` — `VACUUM INTO` `<data>/backups/<name>`
/// over `conn`, gated by [`should_create_backup`]. `Ok(Some(path))` = a backup
/// was taken; `Ok(None)` = skipped (recent) or failed (logged, the partial
/// file removed); `Err` = the gate threw (v4's rejected promise — see the
/// module doc).
///
/// `VACUUM INTO` works on a read-only connection, so the startup backup runs on
/// the read pool; the copy is keyed with the source's key (SQLite3MC).
pub fn create_physical_backup(
    conn: &Connection,
    data_dir: &Path,
    kind: BackupKind,
    now_ms: i64,
    zone: &TimeZone,
) -> Result<Option<PathBuf>, String> {
    if !should_create_backup(data_dir, kind, now_ms, zone)? {
        return Ok(None);
    }

    let dir = backups_dir(data_dir);
    let backup_path = dir.join(generate_backup_filename(kind, now_ms, zone));
    let destination = backup_path.display().to_string();

    let attempt = || -> Result<u64, String> {
        // Ensure the backups directory exists (v4 logs its creation for the
        // main database only; the siblings `mkdir` silently).
        if !exists_sync(&dir) {
            std::fs::create_dir_all(&dir).map_err(|e| node_fs_message(&e, "mkdir", Some(&dir)))?;
            if kind == BackupKind::Main {
                tracing::debug!(
                    target: "quilltap::db",
                    module = PHYSICAL_BACKUP_MODULE,
                    path = dir.display().to_string().as_str(),
                    "Created backups directory"
                );
            }
        }

        tracing::info!(
            target: "quilltap::db",
            module = PHYSICAL_BACKUP_MODULE,
            destination = destination.as_str(),
            "{}",
            kind.starting()
        );

        // Use VACUUM INTO for SQLCipher-compatible backups. The .backup() API
        // creates an unkeyed target file which is incompatible with an
        // encrypted source database. VACUUM INTO preserves the encryption key
        // and creates a consistent, defragmented copy.
        conn.execute_batch(&format!(
            "VACUUM INTO '{}'",
            destination.replace('\'', "''")
        ))
        .map_err(|e| e.to_string())?;

        // Verify the backup file exists and has content
        let size = std::fs::metadata(&backup_path)
            .map_err(|e| node_fs_message(&e, "stat", Some(&backup_path)))?
            .len();
        Ok(size)
    };

    match attempt() {
        Ok(size) => {
            tracing::info!(
                target: "quilltap::db",
                module = PHYSICAL_BACKUP_MODULE,
                path = destination.as_str(),
                sizeBytes = size,
                "{}",
                kind.created()
            );
            Ok(Some(backup_path))
        }
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                module = PHYSICAL_BACKUP_MODULE,
                destination = destination.as_str(),
                error = error.as_str(),
                "{}",
                kind.failed()
            );
            // Clean up partial file on failure
            if exists_sync(&backup_path) {
                match std::fs::remove_file(&backup_path) {
                    Ok(()) => {
                        if kind == BackupKind::Main {
                            tracing::debug!(
                                target: "quilltap::db",
                                module = PHYSICAL_BACKUP_MODULE,
                                path = destination.as_str(),
                                "Cleaned up partial backup file"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            target: "quilltap::db",
                            module = PHYSICAL_BACKUP_MODULE,
                            path = destination.as_str(),
                            error = node_fs_message(&e, "unlink", Some(&backup_path)).as_str(),
                            "{}",
                            kind.cleanup_failed()
                        );
                    }
                }
            }
            Ok(None)
        }
    }
}

/// v4 `applyRetentionPolicy` — per database, keep:
/// - all backups less than 7 days old,
/// - 1 per week for weeks 1-4,
/// - 1 per month for months 1-12,
/// - 1 per year indefinitely,
///
/// and delete the rest.
///
/// Within each weekly/monthly bucket, the OLDEST backup in range is kept so
/// that it has the most runway to age into the next bucket on the following
/// day's retention pass. Picking the newest caused a cascade where every
/// weekly-bucket backup was replaced (and deleted) the day after it first
/// entered the bucket, so nothing ever aged past ~8 days.
///
/// ONE `now_ms` for the three sets (v4 takes a `new Date()` per set,
/// microseconds apart; every comparand is day-granular). Never fails — a
/// retention failure must not affect startup.
pub fn apply_retention_policy(data_dir: &Path, now_ms: i64, zone: &TimeZone) {
    let dir = backups_dir(data_dir);
    if !exists_sync(&dir) {
        return;
    }
    let files = match readdir_sync(&dir) {
        Ok(f) => f,
        Err(error) => {
            tracing::error!(
                target: "quilltap::db",
                module = PHYSICAL_BACKUP_MODULE,
                error = error.as_str(),
                "Failed to apply retention policy"
            );
            // Never throw — retention policy failure should not affect startup
            return;
        }
    };

    // Collect each database's backups separately so retention buckets never
    // cross-delete across databases.
    let mut main: Vec<(String, i64)> = Vec::new();
    let mut llm_logs: Vec<(String, i64)> = Vec::new();
    let mut mount_index: Vec<(String, i64)> = Vec::new();
    for filename in files {
        // Check the more specific patterns first — the main "quilltap-…" regex
        // would otherwise swallow quilltap-llm-logs-… and quilltap-mount-index-…
        // filenames. Actually it won't, because the main regex requires a digit
        // immediately after "quilltap-", but order-of-check is still cheapest.
        if let Some(ms) = parse_backup_filename(BackupKind::MountIndex, &filename, zone) {
            mount_index.push((filename, ms));
            continue;
        }
        if let Some(ms) = parse_backup_filename(BackupKind::LlmLogs, &filename, zone) {
            llm_logs.push((filename, ms));
            continue;
        }
        if let Some(ms) = parse_backup_filename(BackupKind::Main, &filename, zone) {
            main.push((filename, ms));
        }
    }

    for (label, mut items) in [
        ("main", main),
        ("llm-logs", llm_logs),
        ("mount-index", mount_index),
    ] {
        if items.is_empty() {
            continue;
        }
        // Sort newest first (stable, as v4's `Array.prototype.sort`).
        items.sort_by_key(|item| std::cmp::Reverse(item.1));

        // v4's `keep` Set — insertion-ordered, de-duplicated.
        let add = |keep: &mut Vec<String>, name: &str| {
            if !keep.iter().any(|k| k == name) {
                keep.push(name.to_string());
            }
        };
        let mut kept: Vec<String> = Vec::new();

        // Phase 1: Keep all backups < 7 days old
        for (name, ms) in &items {
            if now_ms - ms < 7 * DAY_MS {
                add(&mut kept, name);
            }
        }

        // Phase 2: Keep 1 per week for weeks 1-4 (days 7-28).
        // Iterate oldest-first so the picked backup can age into the next bucket.
        for week in 1..=4i64 {
            let start = 7 * week * DAY_MS;
            let end = 7 * (week + 1) * DAY_MS;
            if let Some((name, _)) = items.iter().rev().find(|(_, ms)| {
                let age = now_ms - ms;
                age >= start && age < end
            }) {
                add(&mut kept, name);
            }
        }

        // Phase 3: Keep 1 per month for months 1-12 (approx days 28-388).
        // Iterate oldest-first for the same reason as Phase 2.
        for month in 1..=12i64 {
            let start = (28 + (month - 1) * 30) * DAY_MS;
            let end = (28 + month * 30) * DAY_MS;
            if let Some((name, _)) = items.iter().rev().find(|(_, ms)| {
                let age = now_ms - ms;
                age >= start && age < end
            }) {
                add(&mut kept, name);
            }
        }

        // Phase 4: Keep 1 per year for anything older than 12 months — the
        // first seen in newest-first order, i.e. each year's NEWEST.
        let mut years: Vec<(i64, &str)> = Vec::new();
        for (name, ms) in &items {
            if now_ms - ms >= 388 * DAY_MS {
                let year = local_parts(*ms, zone).0;
                if !years.iter().any(|(y, _)| *y == year) {
                    years.push((year, name));
                }
            }
        }
        for (_, name) in &years {
            add(&mut kept, name);
        }

        // Delete backups not in the keep set
        let mut deleted = 0u64;
        for (name, _) in &items {
            if kept.iter().any(|k| k == name) {
                continue;
            }
            let path = dir.join(name);
            match std::fs::remove_file(&path) {
                Ok(()) => deleted += 1,
                Err(e) => {
                    tracing::warn!(
                        target: "quilltap::db",
                        module = PHYSICAL_BACKUP_MODULE,
                        filename = name.as_str(),
                        error = node_fs_message(&e, "unlink", Some(&path)).as_str(),
                        "Failed to delete old backup"
                    );
                }
            }
        }

        if deleted > 0 {
            tracing::info!(
                target: "quilltap::db",
                module = PHYSICAL_BACKUP_MODULE,
                database = label,
                totalBackups = items.len() as u64,
                kept = kept.len() as u64,
                deleted,
                "Retention policy applied"
            );
        } else {
            tracing::debug!(
                target: "quilltap::db",
                module = PHYSICAL_BACKUP_MODULE,
                database = label,
                totalBackups = items.len() as u64,
                "Retention policy applied, no backups deleted"
            );
        }
    }
}

/// One sibling backup's outcome in [`run_startup_backups`]: `None` = the
/// partition is unavailable (absent or degraded — v4's client answers no
/// connection, so no backup is called).
fn sibling_backup(
    read: Result<Result<Option<PathBuf>, String>, DbError>,
) -> Option<Result<Option<PathBuf>, String>> {
    match read {
        Ok(outcome) => Some(outcome),
        Err(DbError::PartitionUnavailable(_)) => None,
        Err(e) => Some(Err(crate::db::fallback::error_text(&e))),
    }
}

/// v4 backend.ts `connect()`'s startup backup (`backend.ts:561-568, 580-584,
/// 605-610`), in v4's REAL order. Each backup function is `async` with no
/// `await` before its work, so each runs synchronously at its call — main, then
/// the LLM logs, then the mount index — and the rest are promise reactions:
///
/// ```text
/// createPhysicalBackup(db).then(() => applyRetentionPolicy()).catch(ERROR)
/// createLLMLogsPhysicalBackup(llmLogsDb).catch(ERROR)
/// createMountIndexPhysicalBackup(mountIndexDb).catch(ERROR)
/// ```
///
/// so after the three backups, the microtask queue runs main's `.then`
/// (retention, when main resolved), then the siblings' `.catch`es, and main's
/// `.catch` LAST (its rejection passes through the `.then` first) — measured
/// against v4. The three ERRORs are v4's ROOT logger (no `module`). Runs on the
/// read pool; never fails.
pub fn run_startup_backups(db: &Db, data_dir: &Path, now_ms: i64, zone: &TimeZone) {
    let main = match db.read_main(|c| {
        Ok(create_physical_backup(
            c,
            data_dir,
            BackupKind::Main,
            now_ms,
            zone,
        ))
    }) {
        Ok(outcome) => outcome,
        Err(e) => Err(crate::db::fallback::error_text(&e)),
    };
    let llm_logs = sibling_backup(db.read_llm_logs(|c| {
        Ok(create_physical_backup(
            c,
            data_dir,
            BackupKind::LlmLogs,
            now_ms,
            zone,
        ))
    }));
    let mount_index = sibling_backup(db.read_mount_index(|c| {
        Ok(create_physical_backup(
            c,
            data_dir,
            BackupKind::MountIndex,
            now_ms,
            zone,
        ))
    }));

    if main.is_ok() {
        apply_retention_policy(data_dir, now_ms, zone);
    }
    if let Some(Err(error)) = &llm_logs {
        tracing::error!(
            target: "quilltap::db",
            error = error.as_str(),
            "LLM logs startup physical backup failed"
        );
    }
    if let Some(Err(error)) = &mount_index {
        tracing::error!(
            target: "quilltap::db",
            error = error.as_str(),
            "Mount index startup physical backup failed"
        );
    }
    if let Err(error) = &main {
        tracing::error!(
            target: "quilltap::db",
            error = error.as_str(),
            "Startup physical backup or retention policy failed"
        );
    }
}
