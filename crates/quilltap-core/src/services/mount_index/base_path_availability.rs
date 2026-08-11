//! Base path availability (P4.D67 — v4 `ed8934f1`, Bug 56).
//!
//! A single answer to the question every filesystem-backed store keeps asking:
//! "is my basePath actually there, and if not, whose fault is it?"
//!
//! Three callers need that answer and used to guess at it independently:
//!
//!   - store creation, which warns the operator that scanning will fail;
//!   - folder creation, which must refuse to `mkdir -p` into thin air;
//!   - the scanner, which reports an unreachable mount as an error state.
//!
//! The Docker case deserves its own sentence rather than a generic "not
//! accessible". A container only sees the host paths that were handed to it as
//! bind mounts, so a store whose basePath was perfectly valid on the host is
//! simply absent inside the container — and the remedy (recreate the container
//! with the store bound in) is nothing the operator would infer from ENOENT.
//!
//! Port notes:
//! - v4's `fs.stat` is `std::fs::metadata` here: it FOLLOWS symlinks and it is
//!   the parent directory's search permission that produces EACCES, not the
//!   target's own mode. The differential's `denied` arms plant an unreadable
//!   PARENT for exactly that reason.
//! - v4's `isContainerized()` reads `process.env` at call time. v5 already owns
//!   that pair as pure functions over a [`DataDirEnv`] snapshot
//!   (`services::data_dir`), so [`is_containerized_in`] is the pure half and
//!   [`is_containerized`] is the one impure edge.

use std::path::Path;

use crate::services::data_dir::{is_docker_environment, is_lima_environment, DataDirEnv};

/// Why a base path could not be used (v4 `BasePathUnavailableReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BasePathUnavailableReason {
    /// ENOENT — nothing at that path.
    Missing,
    /// EACCES/EPERM — present but unreadable.
    Denied,
    /// A file (or similar) is sitting where a directory should be.
    NotADirectory,
}

impl BasePathUnavailableReason {
    /// The wire/log spelling v4 uses (`'missing' | 'denied' | 'not-a-directory'`).
    pub fn as_str(self) -> &'static str {
        match self {
            BasePathUnavailableReason::Missing => "missing",
            BasePathUnavailableReason::Denied => "denied",
            BasePathUnavailableReason::NotADirectory => "not-a-directory",
        }
    }
}

/// The unavailable half of v4's `BasePathAvailability` union.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePathUnavailable {
    pub reason: BasePathUnavailableReason,
    /// True when this process is inside a container, making an unbound host
    /// path the likely cause.
    pub containerized: bool,
    /// Operator-facing explanation, safe to surface in the UI.
    pub message: String,
}

/// v4's `BasePathAvailability` union: `{available: true}` or the diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BasePathAvailability {
    Available,
    Unavailable(BasePathUnavailable),
}

impl BasePathAvailability {
    pub fn is_available(&self) -> bool {
        matches!(self, BasePathAvailability::Available)
    }

    /// The diagnosis, or `None` when the path is usable.
    pub fn unavailable(&self) -> Option<&BasePathUnavailable> {
        match self {
            BasePathAvailability::Available => None,
            BasePathAvailability::Unavailable(u) => Some(u),
        }
    }
}

/// v4 `BasePathUnavailableError` — thrown by filesystem mutations that were
/// asked to act on a base path that isn't there. Carries the diagnosis so
/// routes can map it to a sensible status code and message instead of a bare
/// 500. `Display` is v4's `Error.message`, i.e. the diagnosis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePathUnavailableError {
    pub base_path: String,
    pub reason: BasePathUnavailableReason,
    pub containerized: bool,
    pub message: String,
}

impl std::fmt::Display for BasePathUnavailableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BasePathUnavailableError {}

/// v4 `isContainerized()` over a captured env: this process can only see host
/// paths that were explicitly passed through — i.e. a container. Lima counts:
/// it is a VM with the same property.
pub fn is_containerized_in(env: &DataDirEnv) -> bool {
    is_docker_environment(env) || is_lima_environment(env)
}

/// The impure edge: v4 reads `process.env` (and the two filesystem probes) on
/// every call, so we snapshot them on every call too.
pub fn is_containerized() -> bool {
    is_containerized_in(&DataDirEnv::from_process())
}

/// v4's `explain()` — the operator-facing sentences, byte-contractual.
pub fn explain(base_path: &str, reason: BasePathUnavailableReason, containerized: bool) -> String {
    match reason {
        BasePathUnavailableReason::Denied => format!(
            "The path '{base_path}' exists but cannot be read. Check its permissions{}.",
            if containerized {
                ", and note that the container runs as a non-root user"
            } else {
                ""
            }
        ),
        BasePathUnavailableReason::NotADirectory => {
            format!("The path '{base_path}' is not a directory.")
        }
        BasePathUnavailableReason::Missing => {
            if containerized {
                format!(
                    "The path '{base_path}' is not visible from inside the container. \
                     Filesystem document stores must be passed through as bind mounts, which can \
                     only be done when the container is created. Re-run the start script with \
                     `--recreate` to rebuild the container with this store included."
                )
            } else {
                format!("The path '{base_path}' does not exist.")
            }
        }
    }
}

/// The pure core of [`check_base_path_availability`]: the `fs.stat` outcome
/// already reduced to (is-a-directory | errno class), plus the containerized
/// flag. Split out so the container-variant sentences are unit-testable —
/// neither test environment is containerized.
pub fn classify(
    base_path: &str,
    stat: Result<bool, BasePathUnavailableReason>,
    containerized: bool,
) -> BasePathAvailability {
    match stat {
        Ok(true) => BasePathAvailability::Available,
        Ok(false) => BasePathAvailability::Unavailable(BasePathUnavailable {
            reason: BasePathUnavailableReason::NotADirectory,
            containerized,
            message: explain(
                base_path,
                BasePathUnavailableReason::NotADirectory,
                containerized,
            ),
        }),
        Err(reason) => BasePathAvailability::Unavailable(BasePathUnavailable {
            reason,
            containerized,
            message: explain(base_path, reason, containerized),
        }),
    }
}

/// Inspect a mount point's base path and report whether it can be used.
///
/// Never fails: an unreadable path is a diagnosis, not an error. v4's catch
/// arm maps EACCES/EPERM to `denied` and EVERY other errno (ENOENT included,
/// but also ENOTDIR on a mid-path component, ELOOP, …) to `missing`.
pub fn check_base_path_availability(base_path: &str) -> BasePathAvailability {
    let containerized = is_containerized();
    let stat = match std::fs::metadata(Path::new(base_path)) {
        Ok(m) => Ok(m.is_dir()),
        Err(e) => Err(match e.kind() {
            // v4 tests `code === 'EACCES' || code === 'EPERM'`; Rust folds both
            // errnos into `PermissionDenied`.
            std::io::ErrorKind::PermissionDenied => BasePathUnavailableReason::Denied,
            _ => BasePathUnavailableReason::Missing,
        }),
    };
    classify(base_path, stat, containerized)
}

/// v4 `assertBasePathAvailable` — assert that a base path is usable, returning
/// [`BasePathUnavailableError`] if it is not. Use before any filesystem
/// mutation rooted at a mount point.
pub fn assert_base_path_available(base_path: &str) -> Result<(), BasePathUnavailableError> {
    match check_base_path_availability(base_path) {
        BasePathAvailability::Available => Ok(()),
        BasePathAvailability::Unavailable(u) => Err(BasePathUnavailableError {
            base_path: base_path.to_string(),
            reason: u.reason,
            containerized: u.containerized,
            message: u.message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The container-variant sentences are pinned HERE, not in the
    // differential: `containerized` is false in both test environments (no
    // `/.dockerenv`, no `/app` directory, no `LIMA_CONTAINER=true`).
    #[test]
    fn explain_matches_v4_byte_for_byte() {
        assert_eq!(
            explain("/srv/notes", BasePathUnavailableReason::Missing, false),
            "The path '/srv/notes' does not exist."
        );
        assert_eq!(
            explain("/srv/notes", BasePathUnavailableReason::Missing, true),
            "The path '/srv/notes' is not visible from inside the container. Filesystem document \
             stores must be passed through as bind mounts, which can only be done when the \
             container is created. Re-run the start script with `--recreate` to rebuild the \
             container with this store included."
        );
        assert_eq!(
            explain("/srv/notes", BasePathUnavailableReason::Denied, false),
            "The path '/srv/notes' exists but cannot be read. Check its permissions."
        );
        assert_eq!(
            explain("/srv/notes", BasePathUnavailableReason::Denied, true),
            "The path '/srv/notes' exists but cannot be read. Check its permissions, and note \
             that the container runs as a non-root user."
        );
        // not-a-directory carries no container variant in v4.
        assert_eq!(
            explain(
                "/srv/notes",
                BasePathUnavailableReason::NotADirectory,
                false
            ),
            "The path '/srv/notes' is not a directory."
        );
        assert_eq!(
            explain("/srv/notes", BasePathUnavailableReason::NotADirectory, true),
            "The path '/srv/notes' is not a directory."
        );
    }

    #[test]
    fn classify_maps_the_three_arms() {
        assert!(classify("/x", Ok(true), false).is_available());
        let u = classify("/x", Ok(false), false);
        assert_eq!(
            u.unavailable().unwrap().reason,
            BasePathUnavailableReason::NotADirectory
        );
        let u = classify("/x", Err(BasePathUnavailableReason::Denied), true);
        let d = u.unavailable().unwrap();
        assert_eq!(d.reason, BasePathUnavailableReason::Denied);
        assert!(d.containerized);
        assert_eq!(
            d.message,
            explain("/x", BasePathUnavailableReason::Denied, true)
        );
    }

    #[test]
    fn reason_spellings_match_v4() {
        assert_eq!(BasePathUnavailableReason::Missing.as_str(), "missing");
        assert_eq!(BasePathUnavailableReason::Denied.as_str(), "denied");
        assert_eq!(
            BasePathUnavailableReason::NotADirectory.as_str(),
            "not-a-directory"
        );
    }

    #[test]
    fn check_reads_the_real_filesystem() {
        let root = std::env::temp_dir().join(format!("qt-bpa-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dir = root.join("dir");
        std::fs::create_dir_all(&dir).unwrap();
        let file = root.join("file");
        std::fs::write(&file, b"x").unwrap();

        assert!(check_base_path_availability(dir.to_str().unwrap()).is_available());

        let got = check_base_path_availability(file.to_str().unwrap());
        assert_eq!(
            got.unavailable().unwrap().reason,
            BasePathUnavailableReason::NotADirectory
        );

        let missing = root.join("nope");
        let got = check_base_path_availability(missing.to_str().unwrap());
        assert_eq!(
            got.unavailable().unwrap().reason,
            BasePathUnavailableReason::Missing
        );
        assert_eq!(
            got.unavailable().unwrap().message,
            format!("The path '{}' does not exist.", missing.to_str().unwrap())
        );

        // The assert wraps the same diagnosis, carrying the path.
        let err = assert_base_path_available(missing.to_str().unwrap()).unwrap_err();
        assert_eq!(err.base_path, missing.to_str().unwrap());
        assert_eq!(err.to_string(), err.message);
        assert!(assert_base_path_available(dir.to_str().unwrap()).is_ok());

        let _ = std::fs::remove_dir_all(&root);
    }
}
