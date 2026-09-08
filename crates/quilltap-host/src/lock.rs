//! The single-instance lock (P4.1d, v4
//! `lib/database/backends/sqlite/instance-lock.ts`).
//!
//! Prevents two Quilltap processes from opening the same SQLite database
//! simultaneously (WAL/cipher corruption). The lock file lives at
//! `<base>/data/quilltap.lock` and carries JSON with the owning process's PID,
//! hostname, environment type, and a capped history log of state changes.
//!
//! v4's design decisions, all carried over:
//! - **PID-in-file, not `flock()`** — network mounts and bind mounts do not
//!   reliably propagate POSIX file locks.
//! - **Hostname is a human-readable label only. It is NOT proof of machine
//!   identity** (v4 `25f534c0b`, bug 126): macOS derives `gethostname()`
//!   dynamically when `scutil --get HostName` is unset, so one Mac reports
//!   "MacBook-Pro.local" and "Mac" at different times, flipping on Wi-Fi
//!   reconnect, sleep/wake, VPN and DHCP renewal. Ownership is decided by the
//!   snapshot taken when we wrote the lock (PID + `startedAt` —
//!   [`is_still_our_lock`]); liveness of a foreign lock is decided by
//!   heartbeat freshness, for EVERY environment.
//! - **Atomic create** (`O_CREAT | O_EXCL`) for the no-lock fast path; EEXIST
//!   re-reads and falls through to the stale logic.
//! - The **heartbeat** rewrites `lastHeartbeat` (and refreshes the recorded
//!   hostname label) every 60 s; losing ownership (file vanished / a record
//!   that is no longer the one we wrote) is fatal — the host stops its
//!   drivers and, by default, exits the process (v4 closes the DB and
//!   `process.exit(1)`s).
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
//! Environment vocabulary: v4 `1560bd43b` retired `lima`/`wsl2`, leaving
//! `local|electron|docker`. All three are PARSED (a v4-Electron-written lock
//! must classify correctly), plus any retired value via
//! [`EnvironmentType::Other`] — v4 never validated the field, so a pre-4.9
//! `"lima"` lock still reads there and simply fails the container test. v5's
//! own probes only ever EMIT `local|docker` (no Electron shell here — the
//! Tauri host maps later).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::env::{detect_environment_type, EnvironmentType};

/// v4 `HEARTBEAT_INTERVAL_MS` — how often the owner refreshes `lastHeartbeat`.
pub const HEARTBEAT_INTERVAL_MS: u64 = 60_000;

/// v4's different-host freshness window: a container lock with a heartbeat
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

// ============================================================================
// Ownership snapshots (v4 `LockOwnership` / `isStillOurLock`, bug 126)
// ============================================================================

/// Identity of the lock record this process wrote, captured at write time.
///
/// The heartbeat and both releases compare the file against this snapshot
/// rather than against freshly-read process/OS values, so an OS-level
/// hostname change cannot make a process mistake its own lock for someone
/// else's (v4 `25f534c0b`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockOwnership {
    pub pid: u32,
    pub hostname: String,
    pub started_at: String,
}

/// v4 keeps ONE snapshot in `globalThis.__quilltapInstanceLockOwner`, because
/// a Next.js server process holds exactly one instance lock. A v5 host can
/// hold several at once (one per assembled instance) and its unit tests run
/// in parallel inside a single process on distinct paths, so the snapshot is
/// keyed BY LOCK PATH — the same process-global shape, made correct for the
/// several-locks case. (Deviation recorded in the P4.D166 lane record.)
static LOCK_OWNERS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, LockOwnership>>,
> = std::sync::OnceLock::new();

fn lock_owners() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, LockOwnership>> {
    LOCK_OWNERS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// v4 `getLockOwner`.
fn get_lock_owner(lock_path: &Path) -> Option<LockOwnership> {
    lock_owners().lock().unwrap().get(lock_path).cloned()
}

/// v4 `rememberLockOwner(content)` — called at every write of THIS process
/// into the file (fresh create, re-entrant, stale claim, the heartbeat
/// rewrite, and the CLI write-lock's own claims).
fn remember_lock_owner(lock_path: &Path, content: &LockFileContent) {
    lock_owners().lock().unwrap().insert(
        lock_path.to_path_buf(),
        LockOwnership {
            pid: content.pid,
            hostname: content.hostname.clone(),
            started_at: content.started_at.clone(),
        },
    );
}

/// v4 `forgetLockOwner` — called on release.
fn forget_lock_owner(lock_path: &Path) {
    lock_owners().lock().unwrap().remove(lock_path);
}

/// v4 `isStillOurLock(content)`: is the on-disk record still the one this
/// process wrote?
///
/// Compares PID and the acquisition timestamp, both of which any process
/// taking the lock overwrites with its own values. Deliberately does NOT
/// compare hostname: the OS name is not stable over a process's lifetime (see
/// the module header), and a hostname change is not evidence of takeover.
/// Without a snapshot (a lock adopted across a restart of this module's
/// state) PID is all we have — v4's HMR arm.
pub fn is_still_our_lock(lock_path: &Path, content: &LockFileContent) -> bool {
    match get_lock_owner(lock_path) {
        None => content.pid == std::process::id(),
        Some(owner) => content.pid == owner.pid && content.started_at == owner.started_at,
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
    /// container lock with a fresh heartbeat).
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
    let is_container = lock.environment.is_container();
    let heartbeat_age_ms = iso_to_ms(&lock.last_heartbeat)
        .map(|hb| now_ms - hb)
        .unwrap_or(i64::MAX);
    if is_container && heartbeat_age_ms < HEARTBEAT_FRESH_MS {
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

/// v4's same-host conflict label cascade: electron → docker → 'local server'.
/// A retired `lima`/`wsl2` value takes the final arm exactly as v4's
/// unmatched-string fallthrough does.
fn env_label(env: &EnvironmentType) -> &'static str {
    match env {
        EnvironmentType::Electron => "Electron app",
        EnvironmentType::Docker => "Docker container",
        EnvironmentType::Local | EnvironmentType::Other(_) => "local server",
    }
}

/// JS `Math.round`: half UP toward +∞ (`Math.round(-2.5) === -2`), which is
/// `floor(x + 0.5)` — not Rust's `f64::round`, which is half AWAY from zero.
/// Reachable with a negative age whenever a clock skews the heartbeat into the
/// future, which is exactly when the two disagree.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// `${n}` for a JS number: the age terms below are `Infinity` when the record
/// carries no `lastHeartbeat` at all, and V8 prints that word.
fn js_num(x: f64) -> String {
    quilltap_core::jsnum::to_fixed(x, 0)
}

/// `Date.now() - new Date(lastHeartbeat).getTime()`, with v4's
/// `lastHeartbeat ? … : Infinity` guard and V8's `NaN` for an unparseable
/// stamp. Kept in `f64` so both non-finite values survive into the messages.
fn heartbeat_age_ms(existing: &LockFileContent, now_ms: i64) -> f64 {
    if existing.last_heartbeat.is_empty() {
        return f64::INFINITY;
    }
    match iso_to_ms(&existing.last_heartbeat) {
        Some(hb) => (now_ms - hb) as f64,
        None => f64::NAN,
    }
}

/// v4's foreign-host refusal sentence (bug 126). Split out from the cascade so
/// the `wait <N>s` term — which is server-only and so invisible to Tier R —
/// can be pinned byte-for-byte against a frozen age.
fn foreign_fresh_conflict_message(
    existing: &LockFileContent,
    heartbeat_age_ms: f64,
    our_host: &str,
) -> String {
    format!(
        "Another Quilltap instance ({}, PID {} on {}) is already using this database \
         (last heartbeat {}s ago). If no other instance is running, this machine's hostname \
         may have changed since the lock was taken (now: {}); wait {}s for the lock to go \
         stale, or use the lock override to force access.",
        env_label(&existing.environment),
        existing.pid,
        existing.hostname,
        js_num(js_round(heartbeat_age_ms / 1000.0)),
        our_host,
        js_num(((HEARTBEAT_FRESH_MS as f64 - heartbeat_age_ms) / 1000.0).ceil()),
    )
}

/// v4's foreign-host stale-claim reason (bug 126), the twin of the sentence
/// above. Pinned the same way.
fn foreign_stale_claim_reason(existing: &LockFileContent, heartbeat_age_ms: f64) -> String {
    format!(
        "Lock from {} ({}) has no recent heartbeat (last: {}, age: {}s)",
        existing.hostname,
        existing.environment.as_str(),
        if existing.last_heartbeat.is_empty() {
            "never"
        } else {
            existing.last_heartbeat.as_str()
        },
        js_num(js_round(heartbeat_age_ms / 1000.0)),
    )
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

    write_lock_file(lock_path, &content)?;
    remember_lock_owner(lock_path, &content);
    Ok(())
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
                remember_lock_owner(lock_path, &content);
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
        remember_lock_owner(lock_path, &updated);
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

    // Different hostname. This does NOT establish that the lock belongs to a
    // different machine: it is equally likely to be this same machine under a
    // changed OS hostname (see the module header), in which case a live sibling
    // process holds the lock and claiming it would corrupt the database — the
    // exact outcome this module exists to prevent.
    //
    // Since we cannot tell the two cases apart by name, and cannot check PID
    // liveness across a PID namespace, decide on the heartbeat alone — for
    // EVERY environment, not only containers (v4 `25f534c0b`, bug 126):
    // - Recent heartbeat (< HEARTBEAT_FRESH_MS) → someone live holds it, refuse
    // - Stale or missing heartbeat → holder is gone, claim it
    if !same_host {
        let now_ms = iso_to_ms(&now_iso()).unwrap_or(0);
        let age_ms = heartbeat_age_ms(&existing, now_ms);

        if age_ms < HEARTBEAT_FRESH_MS as f64 {
            let message = foreign_fresh_conflict_message(&existing, age_ms, &our_host);
            return Err(LockError::Conflict(Box::new(InstanceLockError {
                message,
                lock_info: existing,
                lock_path: lock_path.to_path_buf(),
            })));
        }

        let stale_reason = foreign_stale_claim_reason(&existing, age_ms);
        return claim_stale_lock(lock_path, existing, stale_reason);
    }

    // A live, different process on the same host — refuse.
    let message = format!(
        "Another Quilltap instance ({}, PID {}) is already using this database. \
         Started at {}. Kill the other process or use the lock override to force access.",
        env_label(&existing.environment),
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
/// vanished, or the record is no longer the one we wrote) — the caller must
/// treat that as fatal (stop the drivers; v4 closes the DB and exits). Write
/// errors are swallowed (v4 debug-logs and keeps the interval running) and
/// report `true`.
///
/// Bug 126: ownership is [`is_still_our_lock`], never a hostname comparison.
/// The recorded hostname is REFRESHED on each write so the file keeps a
/// useful label even when the OS name has since changed.
pub fn heartbeat_tick(lock_path: &Path) -> bool {
    let Some(mut content) = read_lock_file(lock_path) else {
        tracing::error!(
            lockPath = %lock_path.display(),
            "Instance lock file disappeared — another process may claim the database. Shutting down."
        );
        return false; // file disappeared — another process may claim the DB
    };
    if !is_still_our_lock(lock_path, &content) {
        // Hostname is logged for diagnostics but is NOT part of the test.
        let (pid, host, _, _) = our_identity();
        tracing::error!(
            lockPath = %lock_path.display(),
            lockPid = content.pid,
            lockHostname = %content.hostname,
            lockStartedAt = %content.started_at,
            lockEnvironment = %content.environment.as_str(),
            ourPid = pid,
            ourHostname = %host,
            ourStartedAt = %get_lock_owner(lock_path)
                .map(|o| o.started_at)
                .unwrap_or_default(),
            "Instance lock lost — another process has taken over the database. Shutting down."
        );
        return false; // taken over
    }
    content.last_heartbeat = now_iso();
    content.hostname = hostname();
    let _ = write_lock_file(lock_path, &content);
    remember_lock_owner(lock_path, &content);
    tracing::debug!(
        lockPath = %lock_path.display(),
        lastHeartbeat = %content.last_heartbeat,
        "Lock heartbeat updated"
    );
    true
}

/// v4 `releaseInstanceLock`: write a final `released` history entry, then
/// unlink. Owned-by-someone-else / missing / IO errors are all swallowed —
/// never throws (safe in shutdown handlers).
///
/// Bug 126: ownership is [`is_still_our_lock`]. A process whose OS hostname
/// changed under it used to ORPHAN its own lock here.
pub fn release_instance_lock(lock_path: &Path) {
    let Some(existing) = read_lock_file(lock_path) else {
        return;
    };
    let (pid, host, _, _) = our_identity();
    if !is_still_our_lock(lock_path, &existing) {
        tracing::warn!(
            lockPath = %lock_path.display(),
            lockPid = existing.pid,
            lockHostname = %existing.hostname,
            lockStartedAt = %existing.started_at,
            ourPid = pid,
            ourHostname = %host,
            ourStartedAt = %get_lock_owner(lock_path)
                .map(|o| o.started_at)
                .unwrap_or_default(),
            "Lock file not owned by this process, skipping release"
        );
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
    forget_lock_owner(lock_path);
}

/// The lock path for an instance base dir (v4 `getInstanceLockPath()` =
/// `<base>/data/quilltap.lock`).
pub fn instance_lock_path(base_dir: &Path) -> PathBuf {
    base_dir.join("data").join("quilltap.lock")
}

// ============================================================================
// P4.3 additions: the PID-identity probe + the CLI write-lock
// (v4 `packages/quilltap/lib/lock-helpers.js`)
// ============================================================================

/// v4 `verifyPidIsNode` (launcher `lock-helpers.js`): best-effort check that a
/// live PID looks like a Quilltap-shaped process. v4's regex
/// `/node|electron|quilltap|next-server/i` already names `quilltap`, so the v5
/// native binary matches it too — the probe is ported verbatim (a plain
/// case-insensitive substring alternation).
///
/// Per-OS sources, each failing OPEN (`true` — refuse-to-clobber bias):
/// - Linux: `/proc/<pid>/cmdline` first NUL-separated segment.
/// - macOS: `ps -p <pid> -o comm=`.
/// - Windows: `tasklist /FI "PID eq <pid>" /NH`.
/// - anything else: `true`.
pub fn verify_pid_is_quilltap(pid: u32) -> bool {
    fn looks_quilltap(s: &str) -> bool {
        let lower = s.to_lowercase();
        ["node", "electron", "quilltap", "next-server"]
            .iter()
            .any(|n| lower.contains(n))
    }

    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string(format!("/proc/{pid}/cmdline")) {
            Ok(cmdline) => {
                let cmd = cmdline.split('\0').next().unwrap_or("");
                looks_quilltap(cmd)
            }
            Err(_) => true, // Can't read — assume match (v4)
        }
    }
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
        {
            Ok(out) => looks_quilltap(String::from_utf8_lossy(&out.stdout).trim()),
            Err(_) => true,
        }
    }
    #[cfg(target_os = "windows")]
    {
        match std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
        {
            Ok(out) => looks_quilltap(String::from_utf8_lossy(&out.stdout).trim()),
            Err(_) => true,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = pid;
        true
    }
}

/// The launcher's full `getLockStatus` — [`classify_lock_status`] PLUS the
/// same-host PID-identity probe, so a live-but-not-Quilltap-shaped PID
/// classifies as [`LockStatus::Suspect`] (possible PID reuse). This is the
/// classification the CLI lock verbs and the write-lock use; the server-side
/// acquire path deliberately never probes (see [`classify_lock_status`]).
pub fn classify_lock_status_probed(lock_path: &Path, now_ms: i64) -> LockStatus {
    if !lock_path.exists() {
        return LockStatus::Absent;
    }
    let Some(lock) = read_lock_file(lock_path) else {
        return LockStatus::Corrupt;
    };
    let same_host = lock.hostname == hostname();
    if same_host && is_pid_alive(lock.pid) && !verify_pid_is_quilltap(lock.pid) {
        return LockStatus::Suspect {
            reason: format!(
                "PID {} is alive but does not look like a Quilltap process",
                lock.pid
            ),
        };
    }
    classify_lock_content(&lock, now_ms)
}

/// The CLI write-lock refusal/failure (v4 `acquireWriteLock`'s thrown Error).
/// `Display` is the exact v4 message — the CLI prints it verbatim (no
/// `Error:` prefix) and exits 1.
#[derive(Debug)]
pub struct WriteLockError {
    pub message: String,
    /// v4 `err.locked` — true when a live instance holds it.
    pub locked: bool,
}

impl std::fmt::Display for WriteLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for WriteLockError {}

/// v4 launcher `acquireWriteLock(dataDir)` — claim `<dataDir>/quilltap.lock`
/// for a read-write CLI session (`quilltap db --write`), in the very same JSON
/// shape the server writes, so a server starting mid-operation refuses to run.
///
/// Hard rule carried from v4: **NO overrides.** A live lock (`active` or
/// `suspect`) is always refused; only an absent or stale lock is claimed.
/// `data_dir` is the instance's `data/` directory (the lock's own directory),
/// matching the v4 signature.
pub fn acquire_write_lock(data_dir: &Path) -> Result<(), WriteLockError> {
    let lock_path = data_dir.join("quilltap.lock");
    let now_ms = iso_to_ms(&now_iso()).unwrap_or(0);
    let status = classify_lock_status_probed(&lock_path, now_ms);

    fn refuse(status: &LockStatus) -> WriteLockError {
        let (reason, suspect) = match status {
            LockStatus::Active { reason } => (reason.clone(), false),
            LockStatus::Suspect { reason } => (reason.clone(), true),
            _ => unreachable!(),
        };
        let mut lines = vec![format!("Database is currently in use — {reason}.")];
        if !suspect {
            lines.push(
                "Stop the running Quilltap instance before opening the database read-write."
                    .to_string(),
            );
        } else {
            lines.push("This may be a stale lock from a reused PID. Inspect it with".to_string());
            lines.push(
                "`quilltap db --lock-status` and clean it with `quilltap db --lock-clean` if safe."
                    .to_string(),
            );
        }
        lines.push("(See `quilltap db --lock-status` for details.)".to_string());
        WriteLockError {
            message: lines.join("\n"),
            locked: true,
        }
    }

    match &status {
        LockStatus::Active { .. } | LockStatus::Suspect { .. } => return Err(refuse(&status)),
        LockStatus::Corrupt => {
            return Err(WriteLockError {
                message: format!(
                    "Lock file at {} is corrupt. Inspect it manually or clean it with \
                     `quilltap db --lock-clean`, then retry.",
                    lock_path.display()
                ),
                locked: false,
            });
        }
        LockStatus::Stale { reason } => {
            let existing = read_lock_file(&lock_path);
            return claim_stale_write_lock(&lock_path, existing, reason.clone());
        }
        LockStatus::Absent => {}
    }

    // Absent — atomic create so we lose cleanly to any racing process.
    let mut content = build_lock_content();
    add_history_entry(
        &mut content,
        LockEvent::Acquired,
        Some("Read-write CLI session (quilltap db --write)".to_string()),
    );
    let json = serde_json::to_string_pretty(&content).map_err(|e| WriteLockError {
        message: format!("serialize lock: {e}"),
        locked: false,
    })?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut f) => {
            f.write_all(format!("{json}\n").as_bytes())
                .map_err(|e| WriteLockError {
                    message: format!("write lock: {e}"),
                    locked: false,
                })?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Someone created the lock between our check and our create.
            // Re-decide (v4's EEXIST recheck).
            let recheck = classify_lock_status_probed(&lock_path, now_ms);
            match &recheck {
                LockStatus::Active { reason } | LockStatus::Suspect { reason } => {
                    Err(WriteLockError {
                        message: format!(
                            "Database is currently in use — {reason}.\n\
                             Stop the running Quilltap instance before opening the database read-write."
                        ),
                        locked: true,
                    })
                }
                LockStatus::Corrupt => Err(WriteLockError {
                    message: "Database is currently in use — lock just claimed by another \
                              process.\n\
                              Stop the running Quilltap instance before opening the database \
                              read-write."
                        .to_string(),
                    locked: true,
                }),
                LockStatus::Stale { reason } => {
                    claim_stale_write_lock(&lock_path, read_lock_file(&lock_path), reason.clone())
                }
                LockStatus::Absent => claim_stale_write_lock(
                    &lock_path,
                    None,
                    "reclaimed after race".to_string(),
                ),
            }
        }
        Err(e) => Err(WriteLockError {
            message: format!("create lock: {e}"),
            locked: false,
        }),
    }
}

/// v4 launcher `claimStaleLock`: fresh content for THIS process, prior history
/// preserved, `stale-detected` + `stale-claimed` entries appended (the CLI
/// detail flavor).
fn claim_stale_write_lock(
    lock_path: &Path,
    existing: Option<LockFileContent>,
    reason: String,
) -> Result<(), WriteLockError> {
    let mut content = build_lock_content();
    if let Some(prior) = existing {
        content.history = prior.history;
    }
    add_history_entry(&mut content, LockEvent::StaleDetected, Some(reason));
    let (pid, _, _, _) = our_identity();
    add_history_entry(
        &mut content,
        LockEvent::StaleClaimed,
        Some(format!("Claimed by PID {pid} (quilltap db --write)")),
    );
    write_lock_file(lock_path, &content).map_err(|e| WriteLockError {
        message: e.to_string(),
        locked: false,
    })
}

/// v4 launcher `releaseWriteLock(dataDir)`: release iff we own it — write a
/// final `released` history entry, then unlink. Idempotent; never errors.
pub fn release_write_lock(data_dir: &Path) {
    let lock_path = data_dir.join("quilltap.lock");
    let Some(existing) = read_lock_file(&lock_path) else {
        return; // missing or corrupt — don't touch a lock we can't prove is ours
    };
    let (pid, host, _, _) = our_identity();
    if existing.pid != pid || existing.hostname != host {
        return;
    }
    let mut updated = existing;
    add_history_entry(
        &mut updated,
        LockEvent::Released,
        Some(format!("Released by PID {pid} (quilltap db --write)")),
    );
    let _ = write_lock_file(&lock_path, &updated);
    let _ = std::fs::remove_file(&lock_path);
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

    /// `test_support::captured` installs a THREAD-scoped subscriber, and
    /// `tracing` caches each callsite's `Interest` **globally** on first use:
    /// a sibling test that reaches `heartbeat_tick`'s `debug!`/`error!` lines
    /// first, with no subscriber armed on ITS thread, can retire those
    /// callsites for the whole binary (the `global_capture` module doc names
    /// this exact race; it flaked 2 runs in 5 here before this arming). No
    /// global default exists anywhere else in `quilltap-host`, so arming an
    /// empty registry once is enough to keep every callsite interesting —
    /// the per-test thread-local default still takes precedence over it.
    fn captured(f: impl FnOnce()) -> Vec<String> {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry());
        });
        quilltap_core::test_support::captured(f)
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
                // v4 `25f534c0b` rewrote this sentence: the old one ended
                // "Stop the other instance or use the lock override to force
                // access."; the new one names the rename hypothesis and the
                // wait.
                assert!(
                    e.message.starts_with(
                        "Another Quilltap instance (Docker container, PID 4242 on \
                         some-other-host) is already using this database (last heartbeat "
                    ),
                    "{}",
                    e.message
                );
                assert!(
                    e.message.contains(
                        "If no other instance is running, this machine's hostname may have \
                         changed since the lock was taken (now: "
                    ),
                    "{}",
                    e.message
                );
                assert!(
                    e.message.ends_with(
                        "for the lock to go stale, or use the lock override to force access."
                    ),
                    "{}",
                    e.message
                );
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

    /// v4 `25f534c0b`'s inversion of `should claim lock for non-VM different
    /// hostname` → **`should refuse a different hostname with a fresh
    /// heartbeat, whatever the environment`** (bug 126). A differing name
    /// cannot tell another machine from this one after a rename, so a fresh
    /// heartbeat wins over the name in EVERY environment. The old code claimed
    /// any non-docker foreign-hostname lock outright — the fail-open case
    /// where two processes on one machine could both open the database.
    #[test]
    fn foreign_host_fresh_heartbeat_refuses_in_every_environment() {
        for env in [
            EnvironmentType::Local,
            EnvironmentType::Electron,
            EnvironmentType::Docker,
        ] {
            let path = temp_lock_path();
            let mut content = build_lock_content();
            content.pid = 4242;
            content.hostname = "laptop-elsewhere".to_string();
            content.environment = env.clone();
            content.last_heartbeat = now_iso();
            write_lock_file(&path, &content).unwrap();

            match acquire_instance_lock(&path) {
                Err(LockError::Conflict(e)) => {
                    assert!(
                        e.message.contains("laptop-elsewhere"),
                        "{env:?}: {}",
                        e.message
                    );
                    assert!(
                        e.message.contains("hostname may have changed"),
                        "{env:?}: {}",
                        e.message
                    );
                }
                other => panic!("{env:?}: expected conflict, got {other:?}"),
            }
            // Nothing was written: the holder's record stands.
            assert_eq!(read_lock_file(&path).unwrap().pid, 4242);
            let _ = std::fs::remove_file(&path);
        }
    }

    /// v4's twin, **`should claim a different hostname with a stale heartbeat,
    /// whatever the environment`** — the freshness window is no longer gated
    /// on containers, so a stale LOCAL foreign lock is still claimable.
    #[test]
    fn foreign_host_stale_heartbeat_is_claimed_in_every_environment() {
        for env in [
            EnvironmentType::Local,
            EnvironmentType::Electron,
            EnvironmentType::Docker,
        ] {
            let path = temp_lock_path();
            let mut content = build_lock_content();
            content.pid = 4242;
            content.hostname = "laptop-elsewhere".to_string();
            content.environment = env.clone();
            content.last_heartbeat = "2020-01-01T00:00:00.000Z".to_string();
            write_lock_file(&path, &content).unwrap();

            acquire_instance_lock(&path).unwrap();
            let claimed = read_lock_file(&path).unwrap();
            assert_eq!(claimed.pid, std::process::id());
            let detail = claimed.history[0].detail.as_deref().unwrap();
            assert!(
                detail.starts_with(&format!(
                    "Lock from laptop-elsewhere ({}) has no recent heartbeat \
                     (last: 2020-01-01T00:00:00.000Z, age: ",
                    env.as_str()
                )),
                "{env:?}: {detail}"
            );
            release_instance_lock(&path);
        }
    }

    /// v4's refusal sentence, byte-for-byte, over a FROZEN heartbeat age (the
    /// `wait <N>s` term is server-only — it never reaches the CLI, so Tier R
    /// cannot pin it).
    #[test]
    fn foreign_fresh_conflict_message_is_v4s_bytes() {
        let mut existing = build_lock_content();
        existing.pid = 4242;
        existing.hostname = "elsewhere-host".to_string();
        existing.environment = EnvironmentType::Local;
        assert_eq!(
            // 61_600 ms: round(61.6) = 62 for the age, and the wait term is
            // ceil(238.4) = 239 where a Math.round would give 238 — the one
            // frozen age that tells the two apart.
            foreign_fresh_conflict_message(&existing, 61_600.0, "Mac"),
            "Another Quilltap instance (local server, PID 4242 on elsewhere-host) is already \
             using this database (last heartbeat 62s ago). If no other instance is running, \
             this machine's hostname may have changed since the lock was taken (now: Mac); \
             wait 239s for the lock to go stale, or use the lock override to force access."
        );
        existing.environment = EnvironmentType::Docker;
        assert!(foreign_fresh_conflict_message(&existing, 0.0, "Mac")
            .starts_with("Another Quilltap instance (Docker container, PID 4242 on "));
        assert!(foreign_fresh_conflict_message(&existing, 0.0, "Mac").contains("wait 300s for"));
        existing.environment = EnvironmentType::Electron;
        assert!(foreign_fresh_conflict_message(&existing, 1.0, "Mac")
            .starts_with("Another Quilltap instance (Electron app, PID 4242 on "));
    }

    /// v4's stale-claim reason, byte-for-byte, including the `'never'` arm an
    /// empty `lastHeartbeat` takes (`Infinity` rendered as V8 renders it).
    #[test]
    fn foreign_stale_claim_reason_is_v4s_bytes() {
        let mut existing = build_lock_content();
        existing.hostname = "elsewhere-host".to_string();
        existing.environment = EnvironmentType::Docker;
        existing.last_heartbeat = "2020-01-01T00:00:00.000Z".to_string();
        assert_eq!(
            foreign_stale_claim_reason(&existing, 601_400.0),
            "Lock from elsewhere-host (docker) has no recent heartbeat \
             (last: 2020-01-01T00:00:00.000Z, age: 601s)"
        );
        existing.last_heartbeat = String::new();
        assert_eq!(
            foreign_stale_claim_reason(&existing, f64::INFINITY),
            "Lock from elsewhere-host (docker) has no recent heartbeat \
             (last: never, age: Infinitys)"
        );
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

    /// Bug 126 (v4 `25f534c0b`, its `should not shut down when only the OS
    /// hostname has changed`). `os.hostname()` is not stable over a process's
    /// lifetime — macOS derives it dynamically when `scutil --get HostName` is
    /// unset — so a recorded name that no longer matches ours is NOT evidence
    /// of a takeover. The tick keeps the lock, refreshes the label, and
    /// advances `lastHeartbeat`.
    #[test]
    fn heartbeat_survives_a_hostname_change_and_refreshes_the_label() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();
        let ours = read_lock_file(&path).unwrap();

        // The OS renamed the machine under us: same PID, same startedAt, a
        // different recorded name (and a heartbeat old enough to see move).
        let mut renamed = ours.clone();
        renamed.hostname = "Mac".to_string();
        renamed.last_heartbeat = "2020-01-01T00:00:00.000Z".to_string();
        write_lock_file(&path, &renamed).unwrap();

        assert!(
            heartbeat_tick(&path),
            "a renamed host is a label change, not a takeover"
        );
        let after = read_lock_file(&path).unwrap();
        assert_eq!(
            after.hostname,
            hostname(),
            "the tick refreshes the recorded label"
        );
        assert!(
            after.last_heartbeat > renamed.last_heartbeat,
            "the tick advanced lastHeartbeat"
        );
        assert_eq!(after.pid, ours.pid);
        assert_eq!(after.started_at, ours.started_at);

        // And the renamed process still releases its OWN lock (v4's
        // `should still release when only the hostname has changed`).
        let mut renamed_again = read_lock_file(&path).unwrap();
        renamed_again.hostname = "MacBook-Pro.local".to_string();
        write_lock_file(&path, &renamed_again).unwrap();
        release_instance_lock(&path);
        assert!(!path.exists(), "a renamed process releases its own lock");
    }

    /// The other half of bug 126's ownership test: `startedAt` is a comparand,
    /// so a record carrying our PID but someone else's acquisition timestamp
    /// (the PID-reuse shape) IS a takeover. v4 `isStillOurLock`.
    #[test]
    fn heartbeat_reports_loss_when_only_started_at_moved() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();
        let mut taken = read_lock_file(&path).unwrap();
        taken.started_at = "2030-01-01T00:00:00.000Z".to_string();
        write_lock_file(&path, &taken).unwrap();
        assert!(
            !heartbeat_tick(&path),
            "a different startedAt under our PID is a genuine takeover"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// v4's renamed `should skip release when the lock record is another
    /// process's` — keyed on PID + `startedAt`, no longer on hostname.
    #[test]
    fn release_skips_another_processes_record() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();
        let mut theirs = read_lock_file(&path).unwrap();
        theirs.pid = 99_999;
        theirs.started_at = "2020-01-01T00:00:00.000Z".to_string();
        write_lock_file(&path, &theirs).unwrap();

        let lines = captured(|| release_instance_lock(&path));
        assert!(path.exists(), "someone else's record is left alone");
        let warn = lines
            .iter()
            .find(|l| l.contains("Lock file not owned by this process, skipping release"))
            .unwrap_or_else(|| panic!("no skip warning in {lines:?}"));
        assert!(warn.starts_with("WARN quilltap_host::lock"), "{warn}");
        assert!(warn.contains("lockPid=99999"), "{warn}");
        assert!(
            warn.contains("lockStartedAt=2020-01-01T00:00:00.000Z"),
            "{warn}"
        );
        assert!(
            warn.contains(&format!("ourPid={}", std::process::id())),
            "{warn}"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// v4's two heartbeat-loss `error` sentences and their bags (the interval
    /// body's two early returns), plus the per-tick `debug` line.
    #[test]
    fn heartbeat_loss_logs_v4s_sentences() {
        let path = temp_lock_path();
        acquire_instance_lock(&path).unwrap();

        let lines = captured(|| {
            assert!(heartbeat_tick(&path));
        });
        assert!(
            lines.iter().any(|l| l.contains("Lock heartbeat updated")),
            "{lines:?}"
        );

        let mut taken = read_lock_file(&path).unwrap();
        taken.pid = 99_999;
        taken.started_at = "2020-01-01T00:00:00.000Z".to_string();
        taken.environment = EnvironmentType::Docker;
        taken.hostname = "elsewhere-host".to_string();
        write_lock_file(&path, &taken).unwrap();
        let lines = captured(|| {
            assert!(!heartbeat_tick(&path));
        });
        let lost = lines
            .iter()
            .find(|l| {
                l.contains(
                    "Instance lock lost — another process has taken over the database. Shutting down.",
                )
            })
            .unwrap_or_else(|| panic!("no takeover error in {lines:?}"));
        assert!(lost.starts_with("ERROR quilltap_host::lock"), "{lost}");
        for field in [
            "lockPid=99999",
            "lockHostname=elsewhere-host",
            "lockStartedAt=2020-01-01T00:00:00.000Z",
            "lockEnvironment=docker",
            "ourHostname=",
            "ourStartedAt=",
        ] {
            assert!(lost.contains(field), "{field} missing from {lost}");
        }

        std::fs::remove_file(&path).unwrap();
        let lines = captured(|| {
            assert!(!heartbeat_tick(&path));
        });
        assert!(
            lines.iter().any(|l| l.contains(
                "Instance lock file disappeared — another process may claim the database. Shutting down."
            )),
            "{lines:?}"
        );
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
    #[cfg(unix)]
    fn probed_classification_reports_suspect_and_write_lock_refuses() {
        let path = temp_lock_path();
        // Spawn a live non-Quilltap-shaped process ("sleep") and lock under it.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let mut content = build_lock_content();
        content.pid = child.id();
        write_lock_file(&path, &content).unwrap();

        // The un-probed classifier says Active (server behavior)...
        assert!(matches!(
            classify_lock_status(&path, 0),
            LockStatus::Active { .. }
        ));
        // ...the probed one says Suspect.
        match classify_lock_status_probed(&path, 0) {
            LockStatus::Suspect { reason } => {
                assert_eq!(
                    reason,
                    format!(
                        "PID {} is alive but does not look like a Quilltap process",
                        child.id()
                    )
                );
            }
            other => panic!("expected suspect, got {other:?}"),
        }
        // The write lock refuses with v4's exact suspect message shape.
        let data_dir = path.parent().unwrap().to_path_buf();
        let err = acquire_write_lock(&data_dir).unwrap_err();
        assert!(err.locked);
        assert!(err
            .message
            .starts_with("Database is currently in use — PID"));
        assert!(err
            .message
            .contains("This may be a stale lock from a reused PID."));
        assert!(err
            .message
            .ends_with("(See `quilltap db --lock-status` for details.)"));

        let _ = child.kill();
        let _ = child.wait();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    #[cfg(unix)]
    fn write_lock_acquires_claims_stale_and_releases() {
        let path = temp_lock_path();
        let data_dir = path.parent().unwrap().to_path_buf();

        // Fresh acquire writes the CLI acquisition detail.
        acquire_write_lock(&data_dir).unwrap();
        let content = read_lock_file(&path).unwrap();
        assert_eq!(content.pid, std::process::id());
        assert_eq!(
            content.history[0].detail.as_deref(),
            Some("Read-write CLI session (quilltap db --write)")
        );

        // Release writes a released entry then unlinks.
        release_write_lock(&data_dir);
        assert!(!path.exists());

        // A stale (dead-PID) lock is claimed, preserving history.
        let dead = dead_pid();
        let mut stale = build_lock_content();
        stale.pid = dead;
        add_history_entry(&mut stale, LockEvent::Acquired, Some("old".to_string()));
        write_lock_file(&path, &stale).unwrap();
        acquire_write_lock(&data_dir).unwrap();
        let claimed = read_lock_file(&path).unwrap();
        assert_eq!(claimed.pid, std::process::id());
        let events: Vec<LockEvent> = claimed.history.iter().map(|h| h.event).collect();
        assert_eq!(
            events,
            vec![
                LockEvent::Acquired,
                LockEvent::StaleDetected,
                LockEvent::StaleClaimed
            ]
        );
        assert_eq!(
            claimed.history[2].detail.as_deref(),
            Some(
                format!(
                    "Claimed by PID {} (quilltap db --write)",
                    std::process::id()
                )
                .as_str()
            )
        );
        release_write_lock(&data_dir);

        // A live Quilltap-shaped lock (our own test binary's PID — its
        // executable name contains "quilltap", so the probe passes) refuses
        // with the ACTIVE message shape.
        let own = std::process::id();
        let mut live = build_lock_content();
        live.pid = own;
        write_lock_file(&path, &live).unwrap();
        let err = acquire_write_lock(&data_dir).unwrap_err();
        assert!(err.locked);
        assert!(err
            .message
            .contains(&format!("held by PID {own} on this host")));
        assert!(err.message.contains(
            "Stop the running Quilltap instance before opening the database read-write."
        ));
        // Release skips a lock we don't own (foreign PID).
        let mut foreign = read_lock_file(&path).unwrap();
        foreign.pid = own.wrapping_add(1);
        write_lock_file(&path, &foreign).unwrap();
        release_write_lock(&data_dir);
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    /// v4 `1560bd43b` deleted `lima`/`wsl2` from `EnvironmentType`, but v4's
    /// `readLockFile` never validated the field — a pre-4.9 lock written inside
    /// a Lima VM still PARSES there and then fails every `=== 'docker'` test.
    /// v5 must not turn that lock into `Corrupt`: it round-trips through
    /// `Other` and classifies as a different-host non-container, i.e. STALE
    /// whatever its heartbeat says.
    #[test]
    fn a_retired_lima_lock_parses_and_is_not_a_container() {
        let path = temp_lock_path();
        let raw = r#"{"pid": 4242, "hostname": "elsewhere-host",
"startedAt": "2026-01-01T00:00:00.000Z", "lastHeartbeat": "2026-01-01T00:00:00.000Z",
"environment": "lima", "processTitle": "node", "processArgv0": "/usr/bin/node",
"history": []}"#;
        std::fs::write(&path, raw).unwrap();
        let content = read_lock_file(&path).expect("a retired-vocabulary lock still parses");
        assert_eq!(
            content.environment,
            EnvironmentType::Other("lima".to_string())
        );
        assert_eq!(content.environment.as_str(), "lima");
        assert!(!content.environment.is_container());
        // A heartbeat one second old: `docker` would read ACTIVE here; `lima`
        // no longer can.
        let now_ms = iso_to_ms("2026-01-01T00:00:01.000Z").unwrap();
        assert_eq!(
            classify_lock_status(&path, now_ms),
            LockStatus::Stale {
                reason: "held by elsewhere-host but no recent heartbeat".to_string(),
            }
        );
        // And `Other` is a READ shape only — the label cascade falls through to
        // v4's final arm, and the probe never mints one.
        assert_eq!(env_label(&content.environment), "local server");
        assert!(!matches!(
            detect_environment_type(),
            EnvironmentType::Other(_)
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
