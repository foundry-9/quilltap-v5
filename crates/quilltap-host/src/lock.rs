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
//! The file format is v4's `JSON.stringify(content, null, 2) + '\n'` so a v5
//! lock is readable by v4's launcher helpers and vice versa. A FRESH lock is
//! the serialized struct (field order = declaration order = v4's
//! `buildLockContent`); every rewrite of an EXISTING lock edits the parsed,
//! key-order-preserving map instead (P4.D299 R-L1 — v4 rewrites the parsed
//! object, so an assigned key keeps its slot, a new key appends, and keys v5
//! does not model — a newer v4's, or a malformed `port` — ride through
//! untouched). The [`classify_lock_status`] function
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

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::env::{detect_environment_type, EnvironmentType};

/// v4 `HEARTBEAT_INTERVAL_MS` — how often the owner refreshes `lastHeartbeat`.
pub const HEARTBEAT_INTERVAL_MS: u64 = 60_000;

/// v4's different-host freshness window. TWO readers, deliberately different
/// since bug 126 (`25f534c0b`): the acquire cascade applies it to a foreign
/// lock in EVERY environment (a fresh heartbeat refuses, a stale one is
/// claimed), while `classify_lock_content` — mirroring the launcher's
/// UNTOUCHED `lock-helpers.js` — still applies it to container locks only.
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
///
/// The READ is lenient as v4's `readLockFile` is (P4.171 C-6): v4 checks only
/// `pid` (a number), `hostname` (a string) and `history` (an array) — the
/// shape gate [`read_lock_file_raw`] applies first, with v4's WARN — so every
/// other field defaults when missing or mistyped (a string field reads `""`, an
/// absent `environment` reads as an empty [`EnvironmentType::Other`], which
/// like every unrecognized value is neither `docker` nor `electron`), and a
/// history entry v5 cannot type is skipped rather than failing the whole
/// lock. Every REWRITE still edits the raw map (R-L1), so what this view
/// defaults or skips is never written back.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockFileContent {
    pub pid: u32,
    pub hostname: String,
    #[serde(default, deserialize_with = "lenient_string")]
    pub started_at: String,
    #[serde(default, deserialize_with = "lenient_string")]
    pub last_heartbeat: String,
    #[serde(
        default = "unknown_environment",
        deserialize_with = "lenient_environment"
    )]
    pub environment: EnvironmentType,
    #[serde(default, deserialize_with = "lenient_string")]
    pub process_title: String,
    #[serde(default, deserialize_with = "lenient_string")]
    pub process_argv0: String,
    /// v4 bug 239 (`93c5af6ad`): the HTTP port the server listens on, so the
    /// CLI tools that talk to a running server can find it from an instance
    /// name. Absent in locks written before it, by the CLI's own write-lock,
    /// and by a server with no valid port (`--port 0`; the Tauri shell).
    /// Read LENIENTLY ([`lock_port_from_json`]): a malformed value is `None`,
    /// never a parse failure — a strict read would turn a v4-written oddity
    /// into a failed boot (the re-read arm of [`acquire_instance_lock`]) or a
    /// heartbeat shutdown (P4.D299 R-O / R-L2).
    #[serde(
        default,
        deserialize_with = "deserialize_lock_port",
        skip_serializing_if = "Option::is_none"
    )]
    pub port: Option<u16>,
    #[serde(deserialize_with = "lenient_history")]
    pub history: Vec<LockHistoryEntry>,
}

/// A string field of the lenient view: the string, or `""` for any other JSON
/// value (v4 never type-checks it).
fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        _ => String::new(),
    })
}

/// An `environment` that is absent or not a string: an empty
/// [`EnvironmentType::Other`] — v4 compares it with `'docker'` / `'electron'`
/// and nothing else, so anything unrecognized behaves as this does.
fn unknown_environment() -> EnvironmentType {
    EnvironmentType::Other(String::new())
}

fn lenient_environment<'de, D: Deserializer<'de>>(d: D) -> Result<EnvironmentType, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => {
            serde_json::from_value(Value::String(s)).map_err(serde::de::Error::custom)?
        }
        _ => unknown_environment(),
    })
}

/// v4 checks only `Array.isArray(history)`; an entry v5 cannot type is skipped
/// (the raw map keeps it for every rewrite).
fn lenient_history<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<LockHistoryEntry>, D::Error> {
    let entries = Vec::<Value>::deserialize(d)?;
    Ok(entries
        .into_iter()
        .filter_map(|e| serde_json::from_value(e).ok())
        .collect())
}

/// The lock's port, by v4's ONE lenient predicate (`resolveServerPort`,
/// `lock-helpers.js:151-154`; the server writes only values that pass it):
/// `Number.isInteger(p) && p >= 1 && p <= 65535` over the JSON value. `3005`,
/// `3005.0`, `3.005e3` → `3005`; a string, a non-integer, `0`, `70000`, `-1`,
/// `null`, a boolean or an object → `None`. Total: every input yields
/// `Some(valid)` or `None`.
///
/// NOT the predicate `quilltap db --lock-status` prints its `Port:` line by —
/// that one is v4's UNBOUNDED `Number.isInteger(lock.port)` (`quilltap.js:595`),
/// kept in the CLI.
pub fn lock_port_from_json(value: &Value) -> Option<u16> {
    let n = value.as_f64()?;
    (n.is_finite() && n.trunc() == n && (1.0..=65535.0).contains(&n)).then_some(n as u16)
}

fn deserialize_lock_port<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u16>, D::Error> {
    // Any JSON value deserializes into `Value`; the predicate never refuses.
    let value = Value::deserialize(d)?;
    Ok(lock_port_from_json(&value))
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
/// into the INSTANCE lock (fresh create, re-entrant, stale claim, the
/// heartbeat rewrite). NOT the write lock: `25f534c0b` left
/// `packages/quilltap/lib/lock-helpers.js` untouched, so the write lock's
/// claims and its release still compare PID + hostname as v4's launcher does
/// (the P4.D166 lane record's six-site table).
fn remember_lock_owner(lock_path: &Path, content: &LockFileContent) {
    remember_owner(
        lock_path,
        content.pid,
        &content.hostname,
        &content.started_at,
    );
}

/// [`remember_lock_owner`] for a rewrite over the raw map (the identity this
/// process just wrote into it).
fn remember_owner(lock_path: &Path, pid: u32, hostname: &str, started_at: &str) {
    lock_owners().lock().unwrap().insert(
        lock_path.to_path_buf(),
        LockOwnership {
            pid,
            hostname: hostname.to_string(),
            started_at: started_at.to_string(),
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

/// v4 `buildLockContent()` — `port` between `processArgv0` and `history`
/// when given (v4's `...(port !== undefined ? { port } : {})` spread). The
/// server passes its listening port; the CLI write-lock passes `None` (v4's
/// launcher `buildLockContent`, `lock-helpers.js:224-236`, writes none). The
/// `Building instance lock content` DEBUG is the SERVER's alone, so it is
/// emitted at [`acquire_instance_lock`]'s fresh build, not here (R-L5).
fn build_lock_content(port: Option<u16>) -> LockFileContent {
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
        port,
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

/// v4 `addHistoryEntry` over the raw map: `{ ...content, history }` — the
/// `history` key keeps its slot. Only ever called on a map whose typed read
/// succeeded, so `history` is an array.
fn add_raw_history_entry(raw: &mut Map<String, Value>, event: LockEvent, detail: Option<String>) {
    let (pid, host, _, _) = our_identity();
    let entry = LockHistoryEntry {
        event,
        pid,
        hostname: host,
        timestamp: now_iso(),
        detail,
    };
    let entry = serde_json::to_value(entry).expect("a history entry serializes");
    if let Some(Value::Array(history)) = raw.get_mut("history") {
        history.push(entry);
        let len = history.len();
        if len > MAX_HISTORY_ENTRIES {
            history.drain(0..len - MAX_HISTORY_ENTRIES);
        }
    }
}

/// v4's `content.<key> = value` on the parsed object: an existing key keeps
/// its position (`preserve_order`'s `insert`), a new one appends.
fn set_raw(raw: &mut Map<String, Value>, key: &str, value: impl Into<Value>) {
    raw.insert(key.to_string(), value.into());
}

/// v4 `applyListeningPort(content)` — record THIS process's port on a lock
/// being taken over (in place when the claimed lock had a `port`, appended
/// after `history` when it had none), or drop the previous holder's when this
/// process has no valid port. `shift_remove`, never `remove`: the latter is a
/// SWAP-remove that would move the last key into `port`'s slot.
fn apply_listening_port(raw: &mut Map<String, Value>, port: Option<u16>) {
    match port {
        Some(port) => {
            set_raw(raw, "port", port);
            tracing::debug!(
                module = "database:instance-lock",
                port,
                "Recorded listening port on instance lock"
            );
        }
        None => {
            raw.shift_remove("port");
            tracing::debug!(
                module = "database:instance-lock",
                "Recorded listening port on instance lock"
            );
        }
    }
}

// ============================================================================
// File IO
// ============================================================================

/// v4 `readLockFile`: parse, shape-validate (pid number, hostname string,
/// history array — serde covers all three), `None` on missing/corrupt.
pub fn read_lock_file(lock_path: &Path) -> Option<LockFileContent> {
    read_lock_file_raw(lock_path).map(|(typed, _)| typed)
}

/// [`read_lock_file`] plus the parsed map the rewrites edit (R-L1): ONE read
/// of the file, the typed view deciding, the raw map written back.
///
/// v4's two lines (P4.171 C-6): a missing file is silent (`ENOENT`); any other
/// read failure, or text that is not JSON, is the DEBUG `Could not read lock
/// file, treating as absent {lockPath, error}` (`error` = V8's `JSON.parse`
/// message for unparseable text); a parsed value failing the shape gate —
/// `pid` a number, `hostname` a string, `history` an array — is the WARN `Lock
/// file has invalid structure, treating as corrupt {lockPath}`.
fn read_lock_file_raw(lock_path: &Path) -> Option<(LockFileContent, Map<String, Value>)> {
    let could_not_read = |error: String| {
        tracing::debug!(
            module = "database:instance-lock",
            lockPath = %lock_path.display(),
            errorJson = %serde_json::to_string(&error).expect("a string serializes"),
            "Could not read lock file, treating as absent"
        );
    };
    let text = match std::fs::read_to_string(lock_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            could_not_read(e.to_string());
            return None;
        }
    };
    let parsed = match serde_json::from_str::<Value>(&text) {
        Ok(v) => v,
        Err(e) => {
            could_not_read(
                quilltap_core::jsstr::v8_json_parse_message(&text).unwrap_or_else(|| e.to_string()),
            );
            return None;
        }
    };
    // v4 reads `parsed.pid` first: on a JSON `null` that THROWS (caught as the
    // DEBUG arm); on any other non-object the three typeofs fail (the WARN).
    if parsed.is_null() {
        could_not_read("Cannot read properties of null (reading 'pid')".to_string());
        return None;
    }
    let shape_ok = parsed.get("pid").is_some_and(Value::is_number)
        && parsed.get("hostname").is_some_and(Value::is_string)
        && parsed.get("history").is_some_and(Value::is_array);
    let raw = match parsed {
        Value::Object(raw) if shape_ok => raw,
        _ => {
            tracing::warn!(
                module = "database:instance-lock",
                lockPath = %lock_path.display(),
                "Lock file has invalid structure, treating as corrupt"
            );
            return None;
        }
    };
    // Past v4's gate the only refusal left is a `pid` number v5 cannot hold
    // as a `u32` (negative, fractional, huge) — v4 would go on to probe it.
    let typed = serde_json::from_value(Value::Object(raw.clone())).ok()?;
    Some((typed, raw))
}

/// v4 `writeLockFile`: pretty-2-space JSON + trailing newline, atomically via
/// tmp + rename (the tmp is cleaned up on failure). Writes either a fresh
/// [`LockFileContent`] or an edited raw map.
fn write_lock_file<T: Serialize>(lock_path: &Path, content: &T) -> Result<(), LockError> {
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

/// JS `Math.round` — the shared twin in `quilltap_core::jsnum`, since the CLI's
/// `db --lock-status` renders the same age terms (the unification review of
/// 2026-09-09 found the two crates rounding differently). Reachable with a
/// negative age whenever a clock skews the heartbeat into the future, which is
/// exactly when the two roundings disagree.
fn js_round(x: f64) -> f64 {
    quilltap_core::jsnum::math_round(x)
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

/// v4 `claimStaleLock`: `{ ...existing }`, log `stale-detected` + reason,
/// overwrite the identity keys IN PLACE, apply this process's listening port,
/// log `stale-claimed` — every key v4 never assigns (unknown ones included)
/// rides through where it stood (R-L1).
fn claim_stale_lock(
    lock_path: &Path,
    mut raw: Map<String, Value>,
    reason: String,
    port: Option<u16>,
) -> Result<(), LockError> {
    // v4's WARN, from the lock as read: `existing.environment` /
    // `existing.startedAt` are whatever the file holds — dropped when absent
    // (winston drops `undefined`), a string as itself, any other JSON value
    // under the `…Json` convention so the file record keeps v4's JSON type.
    let stale_string = |key: &str| {
        raw.get(key)
            .and_then(Value::as_str)
            .map(tracing::field::display)
    };
    let stale_json = |key: &str| {
        raw.get(key)
            .filter(|v| !v.is_string())
            .map(|v| tracing::field::display(v.to_string()))
    };
    tracing::warn!(
        module = "database:instance-lock",
        lockPath = %lock_path.display(),
        stalePid = raw.get("pid").and_then(|v| v.as_u64()),
        staleHostname = stale_string("hostname"),
        staleEnvironment = stale_string("environment"),
        staleEnvironmentJson = stale_json("environment"),
        staleStartedAt = stale_string("startedAt"),
        staleStartedAtJson = stale_json("startedAt"),
        reason = %reason,
        "Stale instance lock detected, claiming"
    );
    add_raw_history_entry(&mut raw, LockEvent::StaleDetected, Some(reason));

    let (pid, host, title, argv0) = our_identity();
    let started_at = now_iso();
    set_raw(&mut raw, "pid", pid);
    set_raw(&mut raw, "hostname", host.clone());
    set_raw(&mut raw, "startedAt", started_at.clone());
    set_raw(&mut raw, "lastHeartbeat", now_iso());
    set_raw(
        &mut raw,
        "environment",
        serde_json::to_value(detect_environment_type()).expect("environment serializes"),
    );
    set_raw(&mut raw, "processTitle", title);
    set_raw(&mut raw, "processArgv0", argv0);
    apply_listening_port(&mut raw, port);
    add_raw_history_entry(
        &mut raw,
        LockEvent::StaleClaimed,
        Some(format!("Claimed by PID {pid}")),
    );

    write_lock_file(lock_path, &raw)?;
    remember_owner(lock_path, pid, &host, &started_at);
    tracing::info!(
        module = "database:instance-lock",
        lockPath = %lock_path.display(),
        pid,
        "Instance lock acquired after stale claim"
    );
    Ok(())
}

/// v4 `acquireInstanceLock`. Ok = this process owns the lock; the caller runs
/// the heartbeat ([`heartbeat_tick`]) and releases via
/// [`release_instance_lock`]. A live conflict is [`LockError::Conflict`] —
/// the typed boot error the assembly surfaces.
///
/// `port` is this process's listening port (v4's `resolveListeningPort()`;
/// in v5 `quilltap-web --port` when it is 1–65535, else `None` — P4.D299
/// R-L3): written into a fresh lock, applied on a stale claim, and left
/// untouched by the re-entrant path (v4 never assigns it there).
pub fn acquire_instance_lock(lock_path: &Path, port: Option<u16>) -> Result<(), LockError> {
    let mut existing = read_lock_file_raw(lock_path);

    if existing.is_none() {
        // No lock — atomic create (O_CREAT | O_EXCL); EEXIST re-reads and
        // falls through to the stale logic.
        let mut content = build_lock_content(port);
        // v4 `buildLockContent`'s DEBUG — the server's fresh build only (the
        // CLI write-lock shares `build_lock_content` but v4's launcher logs
        // nothing there). `port` is omitted when absent, as winston drops v4's
        // `undefined`.
        match port {
            Some(port) => tracing::debug!(
                module = "database:instance-lock",
                pid = content.pid,
                port,
                "Building instance lock content"
            ),
            None => tracing::debug!(
                module = "database:instance-lock",
                pid = content.pid,
                "Building instance lock content"
            ),
        }
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
                tracing::info!(
                    module = "database:instance-lock",
                    lockPath = %lock_path.display(),
                    pid = content.pid,
                    environment = %content.environment.as_str(),
                    "Instance lock acquired"
                );
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                existing = read_lock_file_raw(lock_path);
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

    let (existing, mut raw) = existing.expect("checked above");
    let (our_pid, our_host, title, argv0) = our_identity();
    let same_host = existing.hostname == our_host;
    let same_pid = existing.pid == our_pid;
    let pid_alive = same_host && is_pid_alive(existing.pid);

    // Re-entrant: same PID on same host (repeated init). Over the raw map:
    // `port` (even a malformed one) and unknown keys stay where they stood.
    if same_host && same_pid {
        add_raw_history_entry(
            &mut raw,
            LockEvent::Acquired,
            Some("Re-entrant acquisition (same PID)".to_string()),
        );
        let started_at = now_iso();
        set_raw(&mut raw, "startedAt", started_at.clone());
        set_raw(
            &mut raw,
            "environment",
            serde_json::to_value(detect_environment_type()).expect("environment serializes"),
        );
        set_raw(&mut raw, "processTitle", title);
        set_raw(&mut raw, "processArgv0", argv0);
        write_lock_file(lock_path, &raw)?;
        remember_owner(lock_path, existing.pid, &existing.hostname, &started_at);
        tracing::debug!(
            module = "database:instance-lock",
            lockPath = %lock_path.display(),
            pid = our_pid,
            "Instance lock re-acquired (same PID)"
        );
        return Ok(());
    }

    // Same host but the PID is dead — definitively stale.
    if same_host && !pid_alive {
        return claim_stale_lock(
            lock_path,
            raw,
            format!("PID {} is no longer running", existing.pid),
            port,
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
        return claim_stale_lock(lock_path, raw, stale_reason, port);
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
///
/// The rewrite edits the raw map (R-L1): only `lastHeartbeat` and `hostname`
/// move; a `port` — malformed or not — and every unknown key are kept as v4's
/// parsed-object rewrite keeps them.
pub fn heartbeat_tick(lock_path: &Path) -> bool {
    let Some((content, mut raw)) = read_lock_file_raw(lock_path) else {
        tracing::error!(
            module = "database:instance-lock",
            lockPath = %lock_path.display(),
            "Instance lock file disappeared — another process may claim the database. Shutting down."
        );
        return false; // file disappeared — another process may claim the DB
    };
    if !is_still_our_lock(lock_path, &content) {
        // Hostname is logged for diagnostics but is NOT part of the test.
        let (pid, host, _, _) = our_identity();
        tracing::error!(
            module = "database:instance-lock",
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
    let last_heartbeat = now_iso();
    let host = hostname();
    set_raw(&mut raw, "lastHeartbeat", last_heartbeat.clone());
    set_raw(&mut raw, "hostname", host.clone());
    let _ = write_lock_file(lock_path, &raw);
    remember_owner(lock_path, content.pid, &host, &content.started_at);
    tracing::debug!(
        module = "database:instance-lock",
        lockPath = %lock_path.display(),
        lastHeartbeat = %last_heartbeat,
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
    let Some((existing, mut raw)) = read_lock_file_raw(lock_path) else {
        return;
    };
    let (pid, host, _, _) = our_identity();
    if !is_still_our_lock(lock_path, &existing) {
        tracing::warn!(
            module = "database:instance-lock",
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
    // v4's `addHistoryEntry(existing, …)` spread, over the raw map (R-L1).
    add_raw_history_entry(
        &mut raw,
        LockEvent::Released,
        Some(format!("Released by PID {pid}")),
    );
    let _ = write_lock_file(lock_path, &raw);
    let _ = std::fs::remove_file(lock_path);
    forget_lock_owner(lock_path);
    tracing::info!(
        module = "database:instance-lock",
        lockPath = %lock_path.display(),
        pid,
        "Instance lock released"
    );
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

    // Absent — atomic create so we lose cleanly to any racing process. v4's
    // launcher `buildLockContent` writes NO `port` (R-L4).
    let mut content = build_lock_content(None);
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
/// detail flavor). It REBUILDS from fresh content (`lock-helpers.js:368-373`),
/// so the previous holder's `port` and unknown keys are dropped — v4's
/// launcher shape, unlike the server's in-place claim.
fn claim_stale_write_lock(
    lock_path: &Path,
    existing: Option<LockFileContent>,
    reason: String,
) -> Result<(), WriteLockError> {
    let mut content = build_lock_content(None);
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
    let Some((existing, mut raw)) = read_lock_file_raw(&lock_path) else {
        return; // missing or corrupt — don't touch a lock we can't prove is ours
    };
    let (pid, host, _, _) = our_identity();
    if existing.pid != pid || existing.hostname != host {
        return;
    }
    // v4's `{ ...lock, history }` spread, over the raw map (R-L1).
    add_raw_history_entry(
        &mut raw,
        LockEvent::Released,
        Some(format!("Released by PID {pid} (quilltap db --write)")),
    );
    let _ = write_lock_file(&lock_path, &raw);
    let _ = std::fs::remove_file(&lock_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lock path under a fresh scratch instance dir. The `TempDir` rides
    /// out with it — bind it for the test's life; it removes the dir on drop.
    fn temp_lock_path() -> (PathBuf, tempfile::TempDir) {
        let dir = tempfile::Builder::new()
            .prefix("qt-lock-test-")
            .tempdir()
            .expect("tempdir");
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        (instance_lock_path(dir.path()), dir)
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
        acquire_instance_lock(&path, None).unwrap(); // same PID → re-entrant
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
        let (path, _scratch) = temp_lock_path();
        let dead = dead_pid();
        let mut content = build_lock_content(None);
        content.pid = dead;
        write_lock_file(&path, &content).unwrap();

        // Classification agrees before the claim.
        match classify_lock_status(&path, 0) {
            LockStatus::Stale { reason } => {
                assert_eq!(reason, format!("PID {dead} is no longer running"))
            }
            other => panic!("expected stale, got {other:?}"),
        }

        acquire_instance_lock(&path, None).unwrap();
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
        let (path, _scratch) = temp_lock_path();
        // PID 1 is always alive (kill → EPERM → alive).
        let mut content = build_lock_content(None);
        content.pid = 1;
        write_lock_file(&path, &content).unwrap();

        match acquire_instance_lock(&path, None) {
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
        let (path, _scratch) = temp_lock_path();
        // A docker lock from another host with a FRESH heartbeat → refuse.
        let mut content = build_lock_content(None);
        content.pid = 4242;
        content.hostname = "some-other-host".to_string();
        content.environment = EnvironmentType::Docker;
        content.last_heartbeat = now_iso();
        write_lock_file(&path, &content).unwrap();

        match acquire_instance_lock(&path, None) {
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
        acquire_instance_lock(&path, None).unwrap();
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
            let (path, _scratch) = temp_lock_path();
            let mut content = build_lock_content(None);
            content.pid = 4242;
            content.hostname = "laptop-elsewhere".to_string();
            content.environment = env.clone();
            content.last_heartbeat = now_iso();
            write_lock_file(&path, &content).unwrap();

            match acquire_instance_lock(&path, None) {
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
            let (path, _scratch) = temp_lock_path();
            let mut content = build_lock_content(None);
            content.pid = 4242;
            content.hostname = "laptop-elsewhere".to_string();
            content.environment = env.clone();
            content.last_heartbeat = "2020-01-01T00:00:00.000Z".to_string();
            write_lock_file(&path, &content).unwrap();

            acquire_instance_lock(&path, None).unwrap();
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
        let mut existing = build_lock_content(None);
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
        let mut existing = build_lock_content(None);
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
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
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();

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
        let (path, _scratch) = temp_lock_path();
        let mut content = build_lock_content(None);
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
        let (path, _scratch) = temp_lock_path();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_lock_file(&path).is_none());
        assert_eq!(classify_lock_status(&path, 0), LockStatus::Corrupt);
        // A corrupt file reads as absent for acquisition... but the atomic
        // create sees EEXIST and re-reads (still unreadable) → the transient
        // error, NOT a clobber (faithful to v4's create-exclusive path).
        assert!(matches!(
            acquire_instance_lock(&path, None),
            Err(LockError::Io(_))
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    #[cfg(unix)]
    fn probed_classification_reports_suspect_and_write_lock_refuses() {
        let (path, _scratch) = temp_lock_path();
        // Spawn a live non-Quilltap-shaped process ("sleep") and lock under it.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let mut content = build_lock_content(None);
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
        let (path, _scratch) = temp_lock_path();
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
        let mut stale = build_lock_content(None);
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
        let mut live = build_lock_content(None);
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
        let (path, _scratch) = temp_lock_path();
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
        let (path, _scratch) = temp_lock_path();
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

    // ========================================================================
    // P4.D299 — v4 bug 239 (`93c5af6ad`): the lock's optional `port`.
    // KEY ORDER is the contract, so every assertion reads the written BYTES
    // (memory note `serde-json-map-eq-ignores-order`), never `Value ==`.
    // ========================================================================

    /// The top-level keys of the lock file, in the file's order.
    fn keys_of(path: &Path) -> Vec<String> {
        let raw = std::fs::read_to_string(path).unwrap();
        let map: Map<String, Value> = serde_json::from_str(&raw).unwrap();
        map.keys().cloned().collect()
    }

    /// The raw text of one top-level key's value (`None` when absent).
    fn raw_value(path: &Path, key: &str) -> Option<String> {
        let raw = std::fs::read_to_string(path).unwrap();
        let map: Map<String, Value> = serde_json::from_str(&raw).unwrap();
        map.get(key).map(|v| serde_json::to_string(v).unwrap())
    }

    const V4_ORDER_WITH_PORT: [&str; 9] = [
        "pid",
        "hostname",
        "startedAt",
        "lastHeartbeat",
        "environment",
        "processTitle",
        "processArgv0",
        "port",
        "history",
    ];

    /// A complete plant (every field the typed read requires) whose `pid`,
    /// `port` and extra keys the caller chooses; `port` is spliced between
    /// `processArgv0` and `history` as v4 writes it.
    fn plant(
        path: &Path,
        pid: u32,
        port: Option<&str>,
        before: &[(&str, &str)],
        after: &[(&str, &str)],
    ) {
        let mut parts: Vec<String> = before
            .iter()
            .map(|(k, v)| format!("  \"{k}\": {v}"))
            .collect();
        parts.push(format!("  \"pid\": {pid}"));
        parts.push(format!("  \"hostname\": \"{}\"", hostname()));
        parts.push("  \"startedAt\": \"2026-01-02T03:04:05.000Z\"".to_string());
        parts.push("  \"lastHeartbeat\": \"2026-01-02T03:04:05.000Z\"".to_string());
        parts.push("  \"environment\": \"local\"".to_string());
        parts.push("  \"processTitle\": \"node\"".to_string());
        parts.push("  \"processArgv0\": \"/usr/bin/node\"".to_string());
        if let Some(port) = port {
            parts.push(format!("  \"port\": {port}"));
        }
        parts.push("  \"history\": []".to_string());
        parts.extend(after.iter().map(|(k, v)| format!("  \"{k}\": {v}")));
        std::fs::write(path, format!("{{\n{}\n}}\n", parts.join(",\n"))).unwrap();
    }

    // ---- v4's four `listening port` jest cases ----

    #[test]
    fn fresh_lock_records_the_port_between_argv0_and_history() {
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, Some(3005)).unwrap();
        assert_eq!(keys_of(&path), V4_ORDER_WITH_PORT);
        assert_eq!(raw_value(&path, "port").as_deref(), Some("3005"));
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\",\n  \"port\": 3005,\n  \"history\": ["),
            "{raw}"
        );
        assert_eq!(read_lock_file(&path).unwrap().port, Some(3005));
        release_instance_lock(&path);
    }

    #[test]
    fn fresh_lock_records_3000() {
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, Some(3000)).unwrap();
        assert_eq!(raw_value(&path, "port").as_deref(), Some("3000"));
        release_instance_lock(&path);
    }

    #[test]
    fn fresh_lock_without_a_port_writes_no_key() {
        let (path, _scratch) = temp_lock_path();
        acquire_instance_lock(&path, None).unwrap();
        assert!(!keys_of(&path).contains(&"port".to_string()));
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn stale_claim_replaces_the_holders_port_in_place() {
        let (path, _scratch) = temp_lock_path();
        plant(&path, dead_pid(), Some("3001"), &[], &[]);
        acquire_instance_lock(&path, Some(3007)).unwrap();
        assert_eq!(keys_of(&path), V4_ORDER_WITH_PORT);
        assert_eq!(raw_value(&path, "port").as_deref(), Some("3007"));
        release_instance_lock(&path);
    }

    // ---- the placements v4's spread produces ----

    #[test]
    #[cfg(unix)]
    fn claiming_a_portless_lock_appends_the_port_after_history() {
        let (path, _scratch) = temp_lock_path();
        plant(&path, dead_pid(), None, &[], &[]);
        acquire_instance_lock(&path, Some(3007)).unwrap();
        let keys = keys_of(&path);
        assert_eq!(keys.last().map(String::as_str), Some("port"));
        assert_eq!(keys[keys.len() - 2], "history");
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn claiming_without_a_port_shift_removes_the_key_keeping_order() {
        let (path, _scratch) = temp_lock_path();
        plant(
            &path,
            dead_pid(),
            Some("3001"),
            &[("aaFirst", "true")],
            &[("zzExtra", "{\"a\": 1}")],
        );
        acquire_instance_lock(&path, None).unwrap();
        // A swap-remove would have moved `zzExtra` into `port`'s slot.
        assert_eq!(
            keys_of(&path),
            [
                "aaFirst",
                "pid",
                "hostname",
                "startedAt",
                "lastHeartbeat",
                "environment",
                "processTitle",
                "processArgv0",
                "history",
                "zzExtra"
            ]
        );
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn unknown_keys_ride_through_claim_reentrant_and_heartbeat() {
        let (path, _scratch) = temp_lock_path();
        plant(
            &path,
            dead_pid(),
            Some("3001"),
            &[("aaFirst", "true")],
            &[("zzExtra", "{\"a\": 1, \"list\": [1, \"two\", null]}")],
        );
        let want = [
            "aaFirst",
            "pid",
            "hostname",
            "startedAt",
            "lastHeartbeat",
            "environment",
            "processTitle",
            "processArgv0",
            "port",
            "history",
            "zzExtra",
        ];
        acquire_instance_lock(&path, Some(3007)).unwrap(); // claim
        assert_eq!(keys_of(&path), want);
        acquire_instance_lock(&path, Some(4000)).unwrap(); // re-entrant
        assert_eq!(keys_of(&path), want);
        assert_eq!(raw_value(&path, "port").as_deref(), Some("3007"));
        assert!(heartbeat_tick(&path));
        assert_eq!(keys_of(&path), want);
        assert_eq!(raw_value(&path, "aaFirst").as_deref(), Some("true"));
        assert_eq!(
            raw_value(&path, "zzExtra").as_deref(),
            Some("{\"a\":1,\"list\":[1,\"two\",null]}")
        );
        release_instance_lock(&path);
        assert!(!path.exists());
    }

    /// The release's rewrite is ONE `add_raw_history_entry` over the raw map
    /// (both releases then unlink the file, so the written record is not
    /// observable through them); this pins that the rewrite keeps every key
    /// and a malformed `port` where they stood.
    #[test]
    fn the_release_rewrite_keeps_every_key_in_place() {
        let mut raw: Map<String, Value> = serde_json::from_str(
            r#"{"aaFirst":true,"pid":1,"hostname":"h","port":"abc","history":[],"zzExtra":{"a":1}}"#,
        )
        .unwrap();
        add_raw_history_entry(&mut raw, LockEvent::Released, Some("Released".to_string()));
        let keys: Vec<&str> = raw.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["aaFirst", "pid", "hostname", "port", "history", "zzExtra"]
        );
        assert_eq!(raw["port"], Value::from("abc"));
        assert_eq!(raw["history"].as_array().unwrap().len(), 1);
    }

    // ---- the lenient read (R-L2) ----

    #[test]
    fn the_port_predicate_is_v4s_lenient_one() {
        let parse = |s: &str| lock_port_from_json(&serde_json::from_str::<Value>(s).unwrap());
        for (text, want) in [
            ("3005", Some(3005)),
            ("3005.0", Some(3005)),
            ("3.005e3", Some(3005)),
            ("1", Some(1)),
            ("65535", Some(65535)),
            ("\"3005\"", None),
            ("3000.5", None),
            ("0", None),
            ("-0", None),
            ("70000", None),
            ("65536", None),
            ("-1", None),
            ("12345678901234567890", None),
            ("1e21", None),
            ("null", None),
            ("true", None),
            ("{}", None),
            ("[]", None),
        ] {
            assert_eq!(parse(text), want, "{text}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_malformed_port_never_fails_the_read_the_heartbeat_or_the_reentrant_path() {
        for bad in ["\"abc\"", "3000.5", "70000", "null", "{}"] {
            let (path, _scratch) = temp_lock_path();
            plant(&path, std::process::id(), Some(bad), &[], &[]);
            let typed = read_lock_file(&path).unwrap_or_else(|| panic!("{bad}: read is None"));
            assert_eq!(typed.port, None, "{bad}");
            let bad_bytes = raw_value(&path, "port").unwrap();
            // Re-entrant (same PID): the boot must not fail, the value stays.
            acquire_instance_lock(&path, Some(3007))
                .unwrap_or_else(|e| panic!("{bad}: re-entrant acquire failed: {e}"));
            assert_eq!(
                raw_value(&path, "port").as_deref(),
                Some(bad_bytes.as_str())
            );
            // The heartbeat keeps it byte-for-byte and does NOT report a loss.
            assert!(heartbeat_tick(&path), "{bad}: heartbeat reported loss");
            assert_eq!(
                raw_value(&path, "port").as_deref(),
                Some(bad_bytes.as_str())
            );
            assert_eq!(keys_of(&path), V4_ORDER_WITH_PORT, "{bad}");
            release_instance_lock(&path);
            assert!(!path.exists(), "{bad}");
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_claim_replaces_a_malformed_port() {
        for bad in ["\"abc\"", "3000.5", "70000", "null", "{}"] {
            let (path, _scratch) = temp_lock_path();
            plant(&path, dead_pid(), Some(bad), &[], &[]);
            acquire_instance_lock(&path, Some(3007)).unwrap();
            assert_eq!(raw_value(&path, "port").as_deref(), Some("3007"), "{bad}");
            assert_eq!(keys_of(&path), V4_ORDER_WITH_PORT, "{bad}");
            release_instance_lock(&path);
        }
    }

    #[test]
    fn a_whole_float_port_reads_as_the_integer() {
        let (path, _scratch) = temp_lock_path();
        plant(&path, 4242, Some("3005.0"), &[], &[]);
        assert_eq!(read_lock_file(&path).unwrap().port, Some(3005));
    }

    #[test]
    #[cfg(unix)]
    fn a_malformed_port_lock_classifies_never_corrupt() {
        let (path, _scratch) = temp_lock_path();
        plant(&path, dead_pid(), Some("\"abc\""), &[], &[]);
        let status = classify_lock_status_probed(&path, 0);
        assert!(
            matches!(status, LockStatus::Stale { .. }),
            "malformed port must classify, got {status:?}"
        );
    }

    // ---- the CLI write-lock writes none (R-L4) ----

    #[test]
    #[cfg(unix)]
    fn the_write_lock_writes_no_port_and_drops_a_prior_holders() {
        let (path, _scratch) = temp_lock_path();
        let data_dir = path.parent().unwrap().to_path_buf();
        acquire_write_lock(&data_dir).unwrap();
        assert!(!keys_of(&path).contains(&"port".to_string()));
        release_write_lock(&data_dir);
        assert!(!path.exists());

        // A stale holder's `port` and unknown keys are DROPPED by the
        // launcher's rebuild-from-fresh claim (`lock-helpers.js:368-373`).
        plant(
            &path,
            dead_pid(),
            Some("3001"),
            &[("aaFirst", "true")],
            &[("zzExtra", "1")],
        );
        acquire_write_lock(&data_dir).unwrap();
        let keys = keys_of(&path);
        assert_eq!(
            keys,
            [
                "pid",
                "hostname",
                "startedAt",
                "lastHeartbeat",
                "environment",
                "processTitle",
                "processArgv0",
                "history"
            ]
        );
        release_write_lock(&data_dir);
    }

    // ---- the two DEBUG lines (R-L5) ----

    fn owned_lines(lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .filter(|l| {
                l.contains("Building instance lock content")
                    || l.contains("Recorded listening port on instance lock")
            })
            .cloned()
            .collect()
    }

    #[test]
    fn the_fresh_build_logs_its_port_and_omits_none() {
        let (path, _scratch) = temp_lock_path();
        let lines = owned_lines(&captured(|| {
            acquire_instance_lock(&path, Some(3005)).unwrap();
        }));
        assert_eq!(lines.len(), 1, "{lines:?}");
        let pid = std::process::id();
        assert!(
            lines[0].starts_with("DEBUG quilltap_host::lock"),
            "{lines:?}"
        );
        assert!(
            lines[0].contains(&format!(
                " module=database:instance-lock pid={pid} port=3005"
            )),
            "{lines:?}"
        );
        release_instance_lock(&path);

        let (path, _scratch) = temp_lock_path();
        let lines = owned_lines(&captured(|| {
            acquire_instance_lock(&path, None).unwrap();
        }));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].contains(&format!(" module=database:instance-lock pid={pid}")),
            "{lines:?}"
        );
        assert!(!lines[0].contains("port="), "{lines:?}");
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn the_claim_logs_the_recorded_port_and_not_the_build_line() {
        let (path, _scratch) = temp_lock_path();
        plant(&path, dead_pid(), Some("3001"), &[], &[]);
        let lines = owned_lines(&captured(|| {
            acquire_instance_lock(&path, Some(3007)).unwrap();
        }));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("Recorded listening port on instance lock"));
        assert!(
            lines[0].contains(" module=database:instance-lock port=3007"),
            "{lines:?}"
        );
        release_instance_lock(&path);

        let (path, _scratch) = temp_lock_path();
        plant(&path, dead_pid(), Some("3001"), &[], &[]);
        let lines = owned_lines(&captured(|| {
            acquire_instance_lock(&path, None).unwrap();
        }));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains(" module=database:instance-lock"));
        assert!(!lines[0].contains("port="), "{lines:?}");
        release_instance_lock(&path);
    }

    #[test]
    #[cfg(unix)]
    fn the_write_lock_logs_neither_line() {
        let (path, _scratch) = temp_lock_path();
        let data_dir = path.parent().unwrap().to_path_buf();
        let lines = owned_lines(&captured(|| {
            acquire_write_lock(&data_dir).unwrap();
            release_write_lock(&data_dir);
            plant(&path, dead_pid(), Some("3001"), &[], &[]);
            acquire_write_lock(&data_dir).unwrap();
            release_write_lock(&data_dir);
        }));
        assert!(lines.is_empty(), "{lines:?}");
    }
}
