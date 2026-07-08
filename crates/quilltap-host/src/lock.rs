//! The single-instance lock (P4.1d, v4
//! `lib/database/backends/sqlite/instance-lock.ts`).
//!
//! Prevents two Quilltap processes from opening the same SQLite database
//! simultaneously (WAL/cipher corruption). The lock file lives at
//! `<base>/data/quilltap.lock` and carries JSON with the owning process's PID,
//! hostname, environment type, and a capped history log of state changes.
//!
//! v4's design decisions, all carried over:
//! - **PID-in-file, not `flock()`** — VirtioFS (Lima) and network mounts do
//!   not reliably propagate POSIX file locks.
//! - **Hostname disambiguates PIDs** across VM/container boundaries; a
//!   different-host lock is judged by heartbeat freshness (< 5 min → live).
//! - **Atomic create** (`O_CREAT | O_EXCL`) for the no-lock fast path; EEXIST
//!   re-reads and falls through to the stale logic.
//! - The **heartbeat** rewrites `lastHeartbeat` every 60 s; losing ownership
//!   (file vanished / foreign content) is fatal — the host stops its drivers
//!   and, by default, exits the process (v4 closes the DB and `process.exit(1)`s).
//!
//! The file format is v4's `JSON.stringify(content, null, 2) + '\n'` (field
//! order preserved by struct declaration order) so a v5 lock is readable by
//! v4's launcher helpers and vice versa. The [`classify_lock_status`] function
//! mirrors the launcher's read-only `getLockStatus`
//! (`packages/quilltap/lib/lock-helpers.js`) vocabulary —
//! `absent|corrupt|active|stale|suspect` — so the P4.3 CLI lock verbs can
//! reuse it (the `suspect` state needs the PID-identity probe, a P4.3
//! concern; the server-side classifier reports `active` there, exactly like
//! the server's own acquire path, which never runs the identity probe).
//!
//! Environment vocabulary: v4's five (`local|electron|docker|lima|wsl2`) are
//! all PARSED (a v4-Electron-written lock must classify correctly); v5's own
//! probes only ever EMIT `local|docker|lima|wsl2` (no Electron shell here —
//! the Tauri host maps later).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::env::{detect_environment_type, EnvironmentType};

/// v4 `HEARTBEAT_INTERVAL_MS` — how often the owner refreshes `lastHeartbeat`.
pub const HEARTBEAT_INTERVAL_MS: u64 = 60_000;

/// v4's different-host freshness window: a VM/container lock with a heartbeat
/// younger than this is treated as live.
pub const HEARTBEAT_FRESH_MS: i64 = 5 * 60 * 1000;

/// v4 `MAX_HISTORY_ENTRIES`.
const MAX_HISTORY_ENTRIES: usize = 50;

// ============================================================================
// The file shape (v4 `LockFileContent`)
// ============================================================================

/// v4 `LockEvent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockEvent {
    #[serde(rename = "acquired")]
    Acquired,
    #[serde(rename = "released")]
    Released,
    #[serde(rename = "stale-detected")]
    StaleDetected,
    #[serde(rename = "stale-claimed")]
    StaleClaimed,
    #[serde(rename = "override")]
    Override,
    #[serde(rename = "override-rejected")]
    OverrideRejected,
}

/// v4 `LockHistoryEntry` (field order = v4's object literal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockHistoryEntry {
    pub event: LockEvent,
    pub pid: u32,
    pub hostname: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// v4 `LockFileContent` (field order = v4's `buildLockContent`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockFileContent {
    pub pid: u32,
    pub hostname: String,
    pub started_at: String,
    pub last_heartbeat: String,
    pub environment: EnvironmentType,
    pub process_title: String,
    pub process_argv0: String,
    pub history: Vec<LockHistoryEntry>,
}

/// The lock-conflict error (v4 `InstanceLockError`): another live process
/// holds the lock. Carries the message + the holder's content for the P4.2
/// startup-status surface.
#[derive(Debug)]
pub struct InstanceLockError {
    pub message: String,
    pub lock_info: LockFileContent,
    pub lock_path: PathBuf,
}

impl std::fmt::Display for InstanceLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for InstanceLockError {}

/// Acquire failures: a live conflict (the typed boot error) or an IO fault.
/// The conflict payload is boxed (it carries the holder's whole lock content —
/// clippy `result_large_err`).
#[derive(Debug)]
pub enum LockError {
    Conflict(Box<InstanceLockError>),
    Io(String),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::Conflict(e) => write!(f, "{e}"),
            LockError::Io(e) => write!(f, "instance lock IO error: {e}"),
        }
    }
}
impl std::error::Error for LockError {}

// ============================================================================
// Identity + PID liveness
// ============================================================================

fn now_iso() -> String {
    quilltap_core::clock::now_iso()
}

fn iso_to_ms(iso: &str) -> Option<i64> {
    quilltap_core::clock::iso_to_ms(iso)
}

/// The OS hostname (v4 `os.hostname()`). Unix `gethostname`; a failure falls
/// back to `"unknown-host"` (never panics on the boot path).
pub fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        // SAFETY: buf is a valid writable buffer of the stated length.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
        if rc == 0 {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            if let Ok(s) = std::str::from_utf8(&buf[..end]) {
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
        "unknown-host".to_string()
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").unwrap_or_else(|_| "unknown-host".to_string())
    }
}

/// v4 `isPidAlive`: signal-0 existence check. EPERM (exists, can't signal) is
/// ALIVE; ESRCH is dead. Non-unix platforms answer conservatively `true`
/// (refuse rather than clobber — the override verbs are P4.3).
pub fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill with signal 0 performs only an existence/permission check.
        let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if rc == 0 {
            return true;
        }
        // EPERM → the process exists but we may not signal it.
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// This process's lock identity (pid / hostname / title / argv0).
fn our_identity() -> (u32, String, String, String) {
    let argv0 = std::env::args().next().unwrap_or_default();
    // v4 writes `process.title` (usually "node"); the closest v5 analogue is
    // the executable's basename.
    let title = Path::new(&argv0)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "quilltap".to_string());
    (std::process::id(), hostname(), title, argv0)
}

/// v4 `buildLockContent()`.
fn build_lock_content() -> LockFileContent {
    let now = now_iso();
    let (pid, host, title, argv0) = our_identity();
    LockFileContent {
        pid,
        hostname: host,
        started_at: now.clone(),
        last_heartbeat: now,
        environment: detect_environment_type(),
        process_title: title,
        process_argv0: argv0,
        history: Vec::new(),
    }
}

/// v4 `addHistoryEntry` — append + trim to the cap.
fn add_history_entry(content: &mut LockFileContent, event: LockEvent, detail: Option<String>) {
    let (pid, host, _, _) = our_identity();
    content.history.push(LockHistoryEntry {
        event,
        pid,
        hostname: host,
        timestamp: now_iso(),
        detail,
    });
    let len = content.history.len();
    if len > MAX_HISTORY_ENTRIES {
        content.history.drain(0..len - MAX_HISTORY_ENTRIES);
    }
}

// ============================================================================
// File IO
// ============================================================================

/// v4 `readLockFile`: parse, shape-validate (pid number, hostname string,
/// history array — serde covers all three), `None` on missing/corrupt.
pub fn read_lock_file(lock_path: &Path) -> Option<LockFileContent> {
    let raw = std::fs::read_to_string(lock_path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// v4 `writeLockFile`: pretty-2-space JSON + trailing newline, atomically via
/// tmp + rename (the tmp is cleaned up on failure).
fn write_lock_file(lock_path: &Path, content: &LockFileContent) -> Result<(), LockError> {
    let tmp = lock_path.with_extension("lock.tmp");
    let json = serde_json::to_string_pretty(content)
        .map_err(|e| LockError::Io(format!("serialize lock: {e}")))?;
    let write =
        std::fs::write(&tmp, format!("{json}\n")).and_then(|_| std::fs::rename(&tmp, lock_path));
    if let Err(e) = write {
        let _ = std::fs::remove_file(&tmp);
        return Err(LockError::Io(format!(
            "write lock {}: {e}",
            lock_path.display()
        )));
    }
    Ok(())
}

// ============================================================================
// Status classification (shared with the P4.3 CLI verbs)
// ============================================================================

/// The launcher's `getLockStatus` vocabulary. The server-side classifier never
/// reaches `Suspect` (the PID-identity probe is a P4.3/launcher concern); it is
/// in the enum so P4.3 extends without reshaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockStatus {
    Absent,
    Corrupt,
    /// Held by a live process (same-host live PID, or a different-host
    /// VM/container lock with a fresh heartbeat).
    Active {
        reason: String,
    },
    /// Claimable: dead same-host PID, or a different-host lock with no fresh
    /// heartbeat.
    Stale {
        reason: String,
    },
    /// P4.3 only: alive PID that does not look like a Quilltap process.
    Suspect {
        reason: String,
    },
}

/// Classify the lock at `lock_path` without modifying it (the launcher's
/// read-only `getLockStatus`, minus the PID-identity probe). `now_ms` anchors
/// the heartbeat-freshness check.
pub fn classify_lock_status(lock_path: &Path, now_ms: i64) -> LockStatus {
    if !lock_path.exists() {
        return LockStatus::Absent;
    }
    let Some(lock) = read_lock_file(lock_path) else {
        return LockStatus::Corrupt;
    };
    classify_lock_content(&lock, now_ms)
}

fn classify_lock_content(lock: &LockFileContent, now_ms: i64) -> LockStatus {
    let same_host = lock.hostname == hostname();
    if same_host {
        if !is_pid_alive(lock.pid) {
            return LockStatus::Stale {
                reason: format!("PID {} is no longer running", lock.pid),
            };
        }
        return LockStatus::Active {
            reason: format!("held by PID {} on this host", lock.pid),
        };
    }
    let is_vm = matches!(
        lock.environment,
        EnvironmentType::Docker | EnvironmentType::Lima | EnvironmentType::Wsl2
    );
    let heartbeat_age_ms = iso_to_ms(&lock.last_heartbeat)
        .map(|hb| now_ms - hb)
        .unwrap_or(i64::MAX);
    if is_vm && heartbeat_age_ms < HEARTBEAT_FRESH_MS {
        return LockStatus::Active {
            reason: format!(
                "held by {} instance on {} (heartbeat {}s ago)",
                lock.environment.as_str(),
                lock.hostname,
                (heartbeat_age_ms as f64 / 1000.0).round() as i64
            ),
        };
    }
    LockStatus::Stale {
        reason: format!("held by {} but no recent heartbeat", lock.hostname),
    }
}

// ============================================================================
// Acquire / release / heartbeat
// ============================================================================

fn env_label(env: EnvironmentType) -> &'static str {
    match env {
        EnvironmentType::Electron => "Electron app",
        EnvironmentType::Docker => "Docker container",
        EnvironmentType::Lima => "Lima VM",
        EnvironmentType::Wsl2 => "WSL2 instance",
        EnvironmentType::Local => "local server",
    }
}

/// v4 `claimStaleLock`: preserve history, log `stale-detected` + reason,
/// overwrite the identity, log `stale-claimed`.
fn claim_stale_lock(
    lock_path: &Path,
    existing: LockFileContent,
    reason: String,
) -> Result<(), LockError> {
    let mut content = existing;
    add_history_entry(&mut content, LockEvent::StaleDetected, Some(reason));

    let (pid, host, title, argv0) = our_identity();
    content.pid = pid;
    content.hostname = host;
    content.started_at = now_iso();
    content.last_heartbeat = now_iso();
    content.environment = detect_environment_type();
    content.process_title = title;
    content.process_argv0 = argv0;
    add_history_entry(
        &mut content,
        LockEvent::StaleClaimed,
        Some(format!("Claimed by PID {pid}")),
    );

    write_lock_file(lock_path, &content)
}

/// v4 `acquireInstanceLock`. Ok = this process owns the lock; the caller runs
/// the heartbeat ([`heartbeat_tick`]) and releases via
/// [`release_instance_lock`]. A live conflict is [`LockError::Conflict`] —
/// the typed boot error the assembly surfaces.
pub fn acquire_instance_lock(lock_path: &Path) -> Result<(), LockError> {
    let mut existing = read_lock_file(lock_path);

    if existing.is_none() {
        // No lock — atomic create (O_CREAT | O_EXCL); EEXIST re-reads and
        // falls through to the stale logic.
        let mut content = build_lock_content();
        add_history_entry(
            &mut content,
            LockEvent::Acquired,
            Some("Clean acquisition — no prior lock".to_string()),
        );
        let json = serde_json::to_string_pretty(&content)
            .map_err(|e| LockError::Io(format!("serialize lock: {e}")))?;

        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
        {
            Ok(mut f) => {
                f.write_all(format!("{json}\n").as_bytes())
                    .map_err(|e| LockError::Io(format!("write lock: {e}")))?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                existing = read_lock_file(lock_path);
                if existing.is_none() {
                    return Err(LockError::Io(format!(
                        "Instance lock file at {} was transiently created by another process \
                         but could not be read. Retry acquisition.",
                        lock_path.display()
                    )));
                }
            }
            Err(e) => return Err(LockError::Io(format!("create lock: {e}"))),
        }
    }

    let existing = existing.expect("checked above");
    let (our_pid, our_host, title, argv0) = our_identity();
    let same_host = existing.hostname == our_host;
    let same_pid = existing.pid == our_pid;
    let pid_alive = same_host && is_pid_alive(existing.pid);

    // Re-entrant: same PID on same host (repeated init).
    if same_host && same_pid {
        let mut updated = existing;
        add_history_entry(
            &mut updated,
            LockEvent::Acquired,
            Some("Re-entrant acquisition (same PID)".to_string()),
        );
        updated.started_at = now_iso();
        updated.environment = detect_environment_type();
        updated.process_title = title;
        updated.process_argv0 = argv0;
        write_lock_file(lock_path, &updated)?;
        return Ok(());
    }

    // Same host but the PID is dead — definitively stale.
    if same_host && !pid_alive {
        return claim_stale_lock(
            lock_path,
            existing.clone(),
            format!("PID {} is no longer running", existing.pid),
        );
    }

    // Different hostname — a VM/container sharing the data dir via a mount.
    // PID liveness can't cross PID namespaces; judge by heartbeat freshness.
    if !same_host {
        let is_vm = matches!(
            existing.environment,
            EnvironmentType::Docker | EnvironmentType::Lima | EnvironmentType::Wsl2
        );
        let now_ms = iso_to_ms(&now_iso()).unwrap_or(0);
        let heartbeat_age_ms = iso_to_ms(&existing.last_heartbeat)
            .map(|hb| now_ms - hb)
            .unwrap_or(i64::MAX);

        if is_vm && heartbeat_age_ms < HEARTBEAT_FRESH_MS {
            let label = match existing.environment {
                EnvironmentType::Docker => "Docker container",
                EnvironmentType::Lima => "Lima VM",
                _ => "WSL2 instance",
            };
            let message = format!(
                "Another Quilltap instance ({label}, PID {} on {}) is already using this \
                 database (last heartbeat {}s ago). Stop the other instance or use the lock \
                 override to force access.",
                existing.pid,
                existing.hostname,
                (heartbeat_age_ms as f64 / 1000.0).round() as i64
            );
            return Err(LockError::Conflict(Box::new(InstanceLockError {
                message,
                lock_info: existing,
                lock_path: lock_path.to_path_buf(),
            })));
        }

        let stale_reason = if is_vm {
            format!(
                "{} lock from {} has no recent heartbeat (last: {}, age: {}s)",
                existing.environment.as_str(),
                existing.hostname,
                if existing.last_heartbeat.is_empty() {
                    "never"
                } else {
                    existing.last_heartbeat.as_str()
                },
                (heartbeat_age_ms as f64 / 1000.0).round() as i64
            )
        } else {
            format!(
                "Different hostname (lock: {}, current: {})",
                existing.hostname, our_host
            )
        };
        return claim_stale_lock(lock_path, existing, stale_reason);
    }

    // A live, different process on the same host — refuse.
    let message = format!(
        "Another Quilltap instance ({}, PID {}) is already using this database. \
         Started at {}. Kill the other process or use the lock override to force access.",
        env_label(existing.environment),
        existing.pid,
        existing.started_at
    );
    Err(LockError::Conflict(Box::new(InstanceLockError {
        message,
        lock_info: existing,
        lock_path: lock_path.to_path_buf(),
    })))
}

/// One heartbeat tick (the body of v4's 60 s `setInterval`): verify ownership
/// and rewrite `lastHeartbeat`. Returns `false` when the lock is LOST (file
/// vanished or foreign content) — the caller must treat that as fatal (stop
/// the drivers; v4 closes the DB and exits). Write errors are swallowed
/// (v4 debug-logs and keeps the interval running) and report `true`.
pub fn heartbeat_tick(lock_path: &Path) -> bool {
    let Some(mut content) = read_lock_file(lock_path) else {
        return false; // file disappeared — another process may claim the DB
    };
    let (pid, host, _, _) = our_identity();
    if content.pid != pid || content.hostname != host {
        return false; // taken over
    }
    content.last_heartbeat = now_iso();
    let _ = write_lock_file(lock_path, &content);
    true
}

/// v4 `releaseInstanceLock`: write a final `released` history entry, then
/// unlink. Owned-by-someone-else / missing / IO errors are all swallowed —
/// never throws (safe in shutdown handlers).
pub fn release_instance_lock(lock_path: &Path) {
    let Some(existing) = read_lock_file(lock_path) else {
        return;
    };
    let (pid, host, _, _) = our_identity();
    if existing.pid != pid || existing.hostname != host {
        return; // not ours — skip
    }
    let mut updated = existing;
    add_history_entry(
        &mut updated,
        LockEvent::Released,
        Some(format!("Released by PID {pid}")),
    );
    let _ = write_lock_file(lock_path, &updated);
    let _ = std::fs::remove_file(lock_path);
}

/// The lock path for an instance base dir (v4 `getInstanceLockPath()` =
/// `<base>/data/quilltap.lock`).
pub fn instance_lock_path(base_dir: &Path) -> PathBuf {
    base_dir.join("data").join("quilltap.lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_lock_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "qt-lock-test-{}-{}",
            std::process::id(),
            uuid_suffix()
        ));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        instance_lock_path(&dir)
    }

    fn uuid_suffix() -> String {
        // Nanos alone can tie across parallel test threads; a process-local
        // counter disambiguates.
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};
        static N: AtomicU64 = AtomicU64::new(0);
        format!(
            "{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            N.fetch_add(1, Ordering::Relaxed)
        )
    }

    /// A PID that is (almost certainly) not running: spawn a child, wait it,
    /// and use its now-dead PID.
    fn dead_pid() -> u32 {
        let child = std::process::Command::new("true")
            .spawn()
            .expect("spawn true");
        let pid = child.id();
        let _ = child.wait_with_output();
        // Give the OS a beat to reap.
        std::thread::sleep(std::time::Duration::from_millis(20));
        pid
    }

    #[test]
    fn fresh_acquire_then_release_unlinks() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();
        let content = read_lock_file(&path).expect("lock written");
        assert_eq!(content.pid, std::process::id());
        assert_eq!(content.hostname, hostname());
        assert_eq!(content.history.len(), 1);
        assert_eq!(content.history[0].event, LockEvent::Acquired);
        // The file is v4's pretty JSON + trailing newline.
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.ends_with('\n'));
        assert!(raw.starts_with("{\n  \"pid\":"));

        release_instance_lock(&path);
        assert!(!path.exists());
        assert_eq!(classify_lock_status(&path, 0), LockStatus::Absent);
    }

    #[test]
    fn reentrant_same_pid_refreshes() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();
        acquire_instance_lock(&path).unwrap(); // same PID → re-entrant
        let content = read_lock_file(&path).unwrap();
        assert_eq!(content.pid, std::process::id());
        assert_eq!(content.history.len(), 2);
        assert_eq!(content.history[1].event, LockEvent::Acquired);
        assert_eq!(
            content.history[1].detail.as_deref(),
            Some("Re-entrant acquisition (same PID)")
        );
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn stale_dead_pid_is_claimed_with_history() {
        let path = temp_lock_path();
        let dead = dead_pid();
        let mut content = build_lock_content();
        content.pid = dead;
        write_lock_file(&path, &content).unwrap();

        // Classification agrees before the claim.
        match classify_lock_status(&path, 0) {
            LockStatus::Stale { reason } => {
                assert_eq!(reason, format!("PID {dead} is no longer running"))
            }
            other => panic!("expected stale, got {other:?}"),
        }

        acquire_instance_lock(&path).unwrap();
        let claimed = read_lock_file(&path).unwrap();
        assert_eq!(claimed.pid, std::process::id());
        let events: Vec<LockEvent> = claimed.history.iter().map(|h| h.event).collect();
        assert_eq!(
            events,
            vec![LockEvent::StaleDetected, LockEvent::StaleClaimed]
        );
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn live_conflict_same_host_refuses() {
        let path = temp_lock_path();
        // PID 1 is always alive (kill → EPERM → alive).
        let mut content = build_lock_content();
        content.pid = 1;
        write_lock_file(&path, &content).unwrap();

        match acquire_instance_lock(&path) {
            Err(LockError::Conflict(e)) => {
                assert!(e.message.contains("Another Quilltap instance"));
                assert!(e.message.contains("PID 1"));
                assert_eq!(e.lock_info.pid, 1);
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        // The refusal leaves the file untouched (still PID 1's).
        assert_eq!(read_lock_file(&path).unwrap().pid, 1);
        // Release skips a lock we don't own.
        release_instance_lock(&path);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn foreign_host_fresh_heartbeat_refuses_stale_claims() {
        let path = temp_lock_path();
        // A docker lock from another host with a FRESH heartbeat → refuse.
        let mut content = build_lock_content();
        content.pid = 4242;
        content.hostname = "some-other-host".to_string();
        content.environment = EnvironmentType::Docker;
        content.last_heartbeat = now_iso();
        write_lock_file(&path, &content).unwrap();

        match acquire_instance_lock(&path) {
            Err(LockError::Conflict(e)) => {
                assert!(e.message.contains("Docker container"));
                assert!(e.message.contains("some-other-host"));
            }
            other => panic!("expected conflict, got {other:?}"),
        }
        assert!(matches!(
            classify_lock_status(&path, iso_to_ms(&now_iso()).unwrap()),
            LockStatus::Active { .. }
        ));

        // Age the heartbeat past 5 minutes → claimable.
        let mut stale = read_lock_file(&path).unwrap();
        stale.last_heartbeat = "2020-01-01T00:00:00.000Z".to_string();
        write_lock_file(&path, &stale).unwrap();
        assert!(matches!(
            classify_lock_status(&path, iso_to_ms(&now_iso()).unwrap()),
            LockStatus::Stale { .. }
        ));
        acquire_instance_lock(&path).unwrap();
        assert_eq!(read_lock_file(&path).unwrap().pid, std::process::id());
        release_instance_lock(&path);
    }

    #[test]
    fn foreign_host_non_vm_is_stale_regardless_of_heartbeat() {
        let path = temp_lock_path();
        // A LOCAL lock from a different hostname is stale even with a fresh
        // heartbeat (v4: only VM/container environments get the freshness test).
        let mut content = build_lock_content();
        content.pid = 4242;
        content.hostname = "laptop-elsewhere".to_string();
        content.environment = EnvironmentType::Local;
        content.last_heartbeat = now_iso();
        write_lock_file(&path, &content).unwrap();

        acquire_instance_lock(&path).unwrap();
        let claimed = read_lock_file(&path).unwrap();
        assert_eq!(claimed.pid, std::process::id());
        assert!(claimed.history[0]
            .detail
            .as_deref()
            .unwrap()
            .starts_with("Different hostname"));
        release_instance_lock(&path);
    }

    #[test]
    fn heartbeat_rewrites_and_detects_loss() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();
        let before = read_lock_file(&path).unwrap().last_heartbeat;
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(heartbeat_tick(&path));
        let after = read_lock_file(&path).unwrap().last_heartbeat;
        assert!(after >= before);

        // Foreign takeover → the tick reports loss.
        let mut foreign = read_lock_file(&path).unwrap();
        foreign.pid = 999_999;
        write_lock_file(&path, &foreign).unwrap();
        assert!(!heartbeat_tick(&path));

        // File vanished → loss.
        std::fs::remove_file(&path).unwrap();
        assert!(!heartbeat_tick(&path));
    }

    #[test]
    fn history_is_capped_at_fifty() {
        let path = temp_lock_path();
        let mut content = build_lock_content();
        for i in 0..60 {
            add_history_entry(&mut content, LockEvent::Acquired, Some(format!("e{i}")));
        }
        assert_eq!(content.history.len(), 50);
        // The oldest entries were trimmed (e10 is now first).
        assert_eq!(content.history[0].detail.as_deref(), Some("e10"));
        write_lock_file(&path, &content).unwrap();
        // Round-trips through the file shape.
        assert_eq!(read_lock_file(&path).unwrap().history.len(), 50);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_lock_classifies_and_reads_none() {
        let path = temp_lock_path();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_lock_file(&path).is_none());
        assert_eq!(classify_lock_status(&path, 0), LockStatus::Corrupt);
        // A corrupt file reads as absent for acquisition... but the atomic
        // create sees EEXIST and re-reads (still unreadable) → the transient
        // error, NOT a clobber (faithful to v4's create-exclusive path).
        assert!(matches!(
            acquire_instance_lock(&path),
            Err(LockError::Io(_))
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn v4_electron_lock_parses() {
        // A v4-written lock (environment 'electron') must classify, not corrupt.
        let path = temp_lock_path();
        let raw = format!(
            r#"{{"pid": 4242, "hostname": "{}", "startedAt": "2026-01-01T00:00:00.000Z",
"lastHeartbeat": "2026-01-01T00:00:00.000Z", "environment": "electron",
"processTitle": "node", "processArgv0": "/usr/bin/node", "history": []}}"#,
            hostname()
        );
        std::fs::write(&path, raw).unwrap();
        let content = read_lock_file(&path).expect("v4 lock parses");
        assert_eq!(content.environment, EnvironmentType::Electron);
        let _ = std::fs::remove_file(&path);
    }
}
