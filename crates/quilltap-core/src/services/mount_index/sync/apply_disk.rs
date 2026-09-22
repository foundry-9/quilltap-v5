//! The disk half of the applier: bytes, sidecars, directories, timestamps.
//!
//! Port of v4 `lib/mount-index/sync/apply-disk.ts` (`23da0b322`).
//!
//! Every write is temp-then-rename, so an interrupted run leaves either the
//! previous file or the new one and never a truncated document the next run
//! would read as an edit. Every path is re-resolved against the target and
//! refused if it escapes — the store's own paths are the input here, and a `..`
//! or an absolute segment in one of them must not be able to write outside the
//! directory the operator named.

use std::fmt;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use super::sidecar::{render_sidecar, sidecar_path_for};
use super::types::{SyncAction, SyncActionKind, DISK_TEMP_SUFFIX};
use crate::clock::iso_to_ms;

/// v4 `DiskPathEscapeError`.
#[derive(Debug, Clone)]
pub struct DiskPathEscapeError {
    pub relative_path: String,
}

impl fmt::Display for DiskPathEscapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Refusing to touch {}: it resolves outside the target directory",
            self.relative_path
        )
    }
}

impl std::error::Error for DiskPathEscapeError {}

/// Anything the disk applier can fail with. The engine turns it into the
/// action's `error` string, which is v4's `err.message`.
#[derive(Debug)]
pub enum DiskApplyError {
    Escape(DiskPathEscapeError),
    Io(std::io::Error),
    /// v4's bare `new Error(...)` for a byte action with no bytes.
    Message(String),
}

impl fmt::Display for DiskApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Escape(e) => e.fmt(f),
            Self::Io(e) => write!(f, "{e}"),
            Self::Message(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for DiskApplyError {}

impl From<std::io::Error> for DiskApplyError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<DiskPathEscapeError> for DiskApplyError {
    fn from(e: DiskPathEscapeError) -> Self {
        Self::Escape(e)
    }
}

/// v4 `resolveInTarget` — resolve a relative path inside the target, refusing
/// anything that escapes.
///
/// Node's `path.resolve` is PURELY LEXICAL: it never touches the filesystem and
/// never resolves a symlink, so the check is a string-prefix one on the
/// normalized form. `std::fs::canonicalize` would be a different (stricter, and
/// failing-on-absent) check, so the normalization is done by hand.
pub fn resolve_in_target(
    target_path: &Path,
    relative_path: &str,
) -> Result<PathBuf, DiskPathEscapeError> {
    let base = lexical_resolve(Path::new(""), target_path);
    let absolute = lexical_resolve(&base, Path::new(relative_path));
    if absolute != base && !absolute.starts_with(&base) {
        return Err(DiskPathEscapeError {
            relative_path: relative_path.to_string(),
        });
    }
    Ok(absolute)
}

/// Node `path.resolve(p)` with one argument — resolve against the process cwd
/// and normalize, with no containment check.
pub fn node_resolve(p: &str) -> PathBuf {
    lexical_resolve(Path::new(""), Path::new(p))
}

/// Node `path.resolve(base, p)` — lexical, `..`-collapsing, absolute-winning.
fn lexical_resolve(base: &Path, p: &Path) -> PathBuf {
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else if base.as_os_str().is_empty() {
        // v4's outer call is `path.resolve(targetPath)`, which resolves against
        // the process cwd. The engine hands an already-absolute target, so cwd
        // only matters for a caller that did not.
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(p)
    } else {
        base.join(p)
    };

    let mut out = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// v4 `targetExists`. A path that exists but is not a directory is reported as
/// absent here and refused by [`ensure_target_directory`]; under `--dry-run`
/// there is nothing to refuse, because nothing will be written.
pub fn target_exists(target_path: &Path) -> bool {
    std::fs::metadata(target_path)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

pub fn ensure_target_directory(target_path: &Path) -> Result<(), DiskApplyError> {
    match std::fs::metadata(target_path) {
        Ok(meta) if !meta.is_dir() => Err(DiskApplyError::Message(format!(
            "{} exists and is not a directory",
            target_path.display()
        ))),
        Ok(_) => Ok(()),
        Err(_) => Ok(std::fs::create_dir_all(target_path)?),
    }
}

/// The two clocks an action can carry.
#[derive(Debug, Clone, Default)]
pub struct DiskTimes<'a> {
    pub last_modified: Option<&'a str>,
    pub created_at: Option<&'a str>,
}

impl<'a> DiskTimes<'a> {
    /// v4 passes the ACTION itself as the `times` bag, so `createdAt: null` and
    /// an absent key both read as "no creation date to set".
    pub fn from_action(action: &'a SyncAction) -> Self {
        Self {
            last_modified: action.last_modified.as_deref(),
            created_at: action.created_at.as_ref().and_then(|v| v.as_deref()),
        }
    }
}

/// v4 `writeDiskFile` — write bytes atomically and stamp the file's clocks.
pub fn write_disk_file(
    target_path: &Path,
    relative_path: &str,
    bytes: &[u8],
    times: &DiskTimes<'_>,
) -> Result<(), DiskApplyError> {
    let absolute = resolve_in_target(target_path, relative_path)?;
    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temp = {
        let mut p = absolute.clone().into_os_string();
        p.push(DISK_TEMP_SUFFIX);
        PathBuf::from(p)
    };
    {
        let mut handle = std::fs::File::create(&temp)?;
        handle.write_all(bytes)?;
        handle.sync_all()?;
    }
    std::fs::rename(&temp, &absolute)?;
    apply_disk_times(&absolute, times)?;
    Ok(())
}

/// v4 `applyDiskTimes` — set a path's clocks.
///
/// Node cannot set birthtime on any platform. On macOS (APFS and HFS+) the
/// kernel *lowers* birthtime to match when `utimes` sets an mtime earlier than
/// the current birthtime, so a two-step — creation date first, then the real
/// mtime — makes Finder and `stat` agree. On Linux birthtime is immutable from
/// user space and on Windows Node has no `SetFileTime`; there the first step is
/// simply a redundant `utimes` and the manifest carries the value instead, so
/// the *comparison* stays right even where the *display* cannot.
pub fn apply_disk_times(absolute_path: &Path, times: &DiskTimes<'_>) -> Result<(), DiskApplyError> {
    // v4 `times.createdAt ? new Date(...) : null` — an empty string is falsy and
    // never reaches `Date`; an unparseable one yields `NaN` and is skipped.
    let created = times
        .created_at
        .filter(|s| !s.is_empty())
        .and_then(iso_to_ms);
    let modified = times
        .last_modified
        .filter(|s| !s.is_empty())
        .and_then(iso_to_ms);

    if let Some(created) = created {
        // v4 `.catch(() => {})` — the creation-date step is advisory.
        let _ = set_times(absolute_path, created);
    }
    if let Some(modified) = modified {
        set_times(absolute_path, modified)?;
    }
    Ok(())
}

/// `fs.utimes(path, when, when)` — atime and mtime both.
fn set_times(path: &Path, ms: i64) -> std::io::Result<()> {
    let when = if ms >= 0 {
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms as u64)
    } else {
        std::time::UNIX_EPOCH - std::time::Duration::from_millis((-ms) as u64)
    };
    let times = std::fs::FileTimes::new()
        .set_accessed(when)
        .set_modified(when);
    std::fs::File::options()
        .write(true)
        .open(path)
        .or_else(|_| std::fs::File::open(path))?
        .set_times(times)
}

/// v4 `birthtimeIsSettable` — true when this platform can be made to report the
/// birthtime we ask for.
pub fn birthtime_is_settable() -> bool {
    // v4 `process.platform === 'darwin'`.
    cfg!(target_os = "macos")
}

/// v4's `process.platform`, for the warning sentence that names it.
pub fn node_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win32"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        std::env::consts::OS
    }
}

pub fn make_disk_directory(
    target_path: &Path,
    relative_path: &str,
    times: &DiskTimes<'_>,
) -> Result<(), DiskApplyError> {
    let absolute = resolve_in_target(target_path, relative_path)?;
    std::fs::create_dir_all(&absolute)?;
    apply_disk_times(&absolute, times)?;
    Ok(())
}

/// v4 `removeDiskFile` — remove a file and the sidecar that belongs to it. A
/// caption with no image is litter, and on the next run it would read as an
/// orphan warning forever.
pub fn remove_disk_file(target_path: &Path, relative_path: &str) -> Result<(), DiskApplyError> {
    let absolute = resolve_in_target(target_path, relative_path)?;
    // `fs.rm(..., { force: true })` — a missing path is not an error.
    let _ = std::fs::remove_file(&absolute);
    let sidecar = resolve_in_target(target_path, &sidecar_path_for(relative_path))?;
    let _ = std::fs::remove_file(sidecar);
    Ok(())
}

/// v4 `removeDiskDirectory` — remove a directory, non-recursively. A directory
/// that is not empty fails loudly rather than taking unplanned content with it.
pub fn remove_disk_directory(
    target_path: &Path,
    relative_path: &str,
) -> Result<(), DiskApplyError> {
    let absolute = resolve_in_target(target_path, relative_path)?;
    match std::fs::remove_dir(&absolute) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(DiskApplyError::Io(e)),
    }
}

/// v4 `writeDiskSidecar` — write (or remove) a binary's sidecar. An empty
/// description removes it.
pub fn write_disk_sidecar(
    target_path: &Path,
    partner_relative_path: &str,
    description: &str,
    description_updated_at: Option<&str>,
) -> Result<(), DiskApplyError> {
    let relative_path = sidecar_path_for(partner_relative_path);
    let body = render_sidecar(description);
    if body.is_empty() {
        let absolute = resolve_in_target(target_path, &relative_path)?;
        let _ = std::fs::remove_file(absolute);
        return Ok(());
    }
    write_disk_file(
        target_path,
        &relative_path,
        body.as_bytes(),
        &DiskTimes {
            last_modified: description_updated_at,
            created_at: None,
        },
    )
}

/// v4 `applyDiskAction` — apply one disk-side action. Bytes, where needed, are
/// supplied by the caller.
pub fn apply_disk_action(
    target_path: &Path,
    action: &SyncAction,
    bytes: Option<&[u8]>,
) -> Result<(), DiskApplyError> {
    let times = DiskTimes::from_action(action);
    match action.kind {
        SyncActionKind::Mkdir => make_disk_directory(target_path, &action.relative_path, &times),
        SyncActionKind::Create | SyncActionKind::Modify => {
            let Some(bytes) = bytes else {
                return Err(DiskApplyError::Message(format!(
                    "No bytes supplied for {} {}",
                    action.kind.as_str(),
                    action.relative_path
                )));
            };
            write_disk_file(target_path, &action.relative_path, bytes, &times)
        }
        SyncActionKind::Touch => {
            let absolute = resolve_in_target(target_path, &action.relative_path)?;
            apply_disk_times(&absolute, &times)
        }
        SyncActionKind::Describe => write_disk_sidecar(
            target_path,
            &action.relative_path,
            action.description.as_deref().unwrap_or(""),
            action.last_modified.as_deref(),
        ),
        SyncActionKind::Delete => remove_disk_file(target_path, &action.relative_path),
        SyncActionKind::Rmdir => remove_disk_directory(target_path, &action.relative_path),
        other => {
            tracing::debug!(
                target: "quilltap::mount_index",
                kind = %other.as_str(),
                "[Sync] Disk applier ignoring action",
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `resolve_in_target`'s escape refusal is DEFENCE IN DEPTH: neither walk can
    /// hand the applier an escaping path (the store walk drops any path with a
    /// dot-segment, and `..` is one; the disk walk only ever reports paths it
    /// just read from inside the target), so no engine scenario can reach it and
    /// the corpus mutation the P4.D210 order names — "let `resolve_in_target`
    /// accept `..`" — would survive `sync_engine_equivalence` untouched. It is
    /// pinned here instead, which is the honest place for a guard whose whole
    /// job is to catch a caller that should not exist.
    #[test]
    fn a_path_that_escapes_the_target_is_refused() {
        let target = Path::new("/tmp/qt-target");
        assert_eq!(
            resolve_in_target(target, "lore/harbour.png").unwrap(),
            Path::new("/tmp/qt-target/lore/harbour.png")
        );
        // A `..` that stays inside is fine — v4 resolves lexically and only then
        // tests containment.
        assert_eq!(
            resolve_in_target(target, "lore/../harbour.png").unwrap(),
            Path::new("/tmp/qt-target/harbour.png")
        );
        for escaping in ["../outside.md", "lore/../../outside.md", "/etc/passwd"] {
            let err =
                resolve_in_target(target, escaping).expect_err("an escaping path must be refused");
            assert_eq!(
                err.to_string(),
                format!("Refusing to touch {escaping}: it resolves outside the target directory")
            );
        }
        // The target ITSELF resolves, which is what `absolute !== base` allows.
        assert_eq!(resolve_in_target(target, "").unwrap(), target);
        // …and a sibling whose name merely STARTS with the target's does not.
        assert!(resolve_in_target(target, "../qt-target-other/x.md").is_err());
    }
}
