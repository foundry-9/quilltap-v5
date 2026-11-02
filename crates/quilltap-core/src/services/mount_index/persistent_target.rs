//! The persistent-target check — v4 `lib/mount-index/persistent-target.ts`
//! (`faeae2b71`). Contract C7.
//!
//! Under Docker, a store written into the container's own layer (the overlay
//! root, a tmpfs, `/proc` …) is lost when the container is recreated: v4's
//! motivating bug was a deconvert to `/tmp/qt-deconvert-test` that succeeded,
//! deleted the database copy, and left the store's only copy in the overlay,
//! where the next `--recreate` destroyed it. Sync (and, in v4, deconvert)
//! therefore refuse a target whose containing mount is ephemeral.
//!
//! The mount table comes through the [`MountInfoSource`] seam (the
//! [`ProcMountInfo`] default reads `/proc/self/mountinfo`) so the refusal is
//! testable off Linux; the `is_docker` input is v4's SERVER probe
//! (`services::data_dir::is_docker_environment`, WITH the `/app`-directory
//! heuristic — v4's `lib/paths.ts` `isDockerEnvironment()` this module
//! imports), read by the CALLER and passed in (P4.D354 R-4).
//!
//! `/proc/self/mountinfo` has no macOS / Windows analogue; the check is
//! Docker-only by v4's own gate, so no platform arm is needed — outside a
//! container the gate returns before the table is read.
//!
//! [`assert_persistent_target`] (v4's deconvert guard) has NO reachable
//! production caller in v5: `mount_deconvert` is the P4.6y refusal
//! (`api/mount_files.rs`). It lands so the future deconvert order calls it;
//! its WARN `Refusing write to a path inside the container layer` is pinned by
//! a unit test and is live only through that order.

use std::path::{Path, PathBuf};

use super::sync::apply_disk::node_resolve;

/// v4 `MOUNTINFO_PATH`.
pub const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";

/// v4 `EPHEMERAL_FS_TYPES`: filesystems that do not outlive the container.
pub const EPHEMERAL_FS_TYPES: &[&str] = &[
    "overlay", "tmpfs", "devtmpfs", "proc", "sysfs", "devpts", "mqueue", "cgroup", "cgroup2",
];

/// v4 `MountEntry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MountEntry {
    pub mount_point: String,
    pub fs_type: String,
}

/// v4 `PersistentTargetCheck`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistentTargetCheck {
    Persistent,
    Ephemeral {
        /// The mount that would hold the files.
        mount_point: String,
        fs_type: String,
        /// Operator-facing explanation, safe to surface in the UI.
        message: String,
    },
}

/// v4 `EphemeralTargetError` (`name = 'EphemeralTargetError'`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EphemeralTargetError {
    pub target_path: String,
    pub message: String,
}

impl std::fmt::Display for EphemeralTargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EphemeralTargetError {}

/// Where the mount table comes from (contract C7).
pub trait MountInfoSource {
    fn read_mount_info(&self) -> std::io::Result<String>;
}

/// The production source: `/proc/self/mountinfo` (v4's default
/// `readFile(MOUNTINFO_PATH, 'utf8')`).
pub struct ProcMountInfo;

impl MountInfoSource for ProcMountInfo {
    fn read_mount_info(&self) -> std::io::Result<String> {
        std::fs::read_to_string(MOUNTINFO_PATH)
    }
}

/// v4 `unescapeMountField`: `\ooo` (three octal digits) → that character
/// (`String.fromCharCode(parseInt(oct, 8))` — every value is < 512, so one
/// UTF-16 unit, which `char::from_u32` reproduces).
fn unescape_mount_field(field: &str) -> String {
    let chars: Vec<char> = field.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\'
            && i + 3 < chars.len()
            && chars[i + 1..=i + 3].iter().all(|c| ('0'..='7').contains(c))
        {
            let oct: String = chars[i + 1..=i + 3].iter().collect();
            let code = u32::from_str_radix(&oct, 8).unwrap_or(0);
            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            i += 4;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// v4 `parseMountInfo(text)`: per non-blank line, `split(' ')`, the `-`
/// separator; skip when there is none, fewer than five fields, or no field
/// after it. `mountPoint` is field 4 (unescaped), `fsType` the field after
/// the separator.
pub fn parse_mount_info(text: &str) -> Vec<MountEntry> {
    let mut entries = Vec::new();
    for line in text.split('\n') {
        if crate::jsstr::js_trim(line).is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(' ').collect();
        let Some(separator) = fields.iter().position(|f| *f == "-") else {
            continue;
        };
        if fields.len() < 5 || separator + 1 >= fields.len() {
            continue;
        }
        entries.push(MountEntry {
            mount_point: unescape_mount_field(fields[4]),
            fs_type: fields[separator + 1].to_string(),
        });
    }
    entries
}

/// v4 `findContainingMount(mounts, absPath)`: a mount contains the path when it
/// is `/`, the path itself, or a `/`-bounded prefix; the LONGEST mount point
/// wins and ties go to the LAST (`>=`).
pub fn find_containing_mount<'a>(
    mounts: &'a [MountEntry],
    abs_path: &str,
) -> Option<&'a MountEntry> {
    let mut best: Option<&MountEntry> = None;
    for mount in mounts {
        let mp = mount.mount_point.as_str();
        let prefix = if mp.ends_with('/') {
            mp.to_string()
        } else {
            format!("{mp}/")
        };
        let contains = mp == "/" || abs_path == mp || abs_path.starts_with(&prefix);
        if !contains {
            continue;
        }
        // JS `.length` — UTF-16 code units.
        let len = crate::jsstr::utf16_len(mp);
        if best.is_none_or(|b| len >= crate::jsstr::utf16_len(&b.mount_point)) {
            best = Some(mount);
        }
    }
    best
}

/// v4 `resolveThroughExistingAncestor(absPath)`: walk up until `fs.realpath`
/// succeeds, then re-append the pending basenames; reaching the root answers
/// the original path.
pub fn resolve_through_existing_ancestor(abs_path: &Path) -> PathBuf {
    let mut pending: Vec<std::ffi::OsString> = Vec::new();
    let mut current = abs_path.to_path_buf();
    loop {
        if let Ok(real) = std::fs::canonicalize(&current) {
            let mut out = real;
            for name in pending.iter().rev() {
                out.push(name);
            }
            return out;
        }
        let Some(parent) = current.parent().map(Path::to_path_buf) else {
            return abs_path.to_path_buf();
        };
        if parent == current {
            return abs_path.to_path_buf();
        }
        match current.file_name() {
            Some(name) => pending.push(name.to_os_string()),
            None => return abs_path.to_path_buf(),
        }
        current = parent;
    }
}

/// The operator message, byte-exact (v4 `persistent-target.ts`).
pub fn ephemeral_target_message(target_path: &str) -> String {
    format!(
        "The path '{target_path}' is inside the container itself, not on a bind mount, so \
         anything written there is lost when the container is recreated. Choose a path under \
         the data directory (/app/quilltap) or under a store that is bind-mounted from the host \
         (see 'quilltap docs docker-mounts')."
    )
}

/// v4 `checkPersistentTarget(targetPath, readMountInfo)`.
pub fn check_persistent_target(
    target_path: &str,
    is_docker: bool,
    source: &dyn MountInfoSource,
) -> PersistentTargetCheck {
    if !is_docker {
        return PersistentTargetCheck::Persistent;
    }
    let mounts = match source.read_mount_info() {
        Ok(text) => parse_mount_info(&text),
        Err(e) => {
            tracing::warn!(
                target: "quilltap::mount_index",
                targetPath = %target_path,
                error = %e,
                "Could not read the mount table; not checking target persistence"
            );
            return PersistentTargetCheck::Persistent;
        }
    };
    let resolved = resolve_through_existing_ancestor(&node_resolve(target_path));
    let resolved = resolved.to_string_lossy().into_owned();
    let Some(mount) = find_containing_mount(&mounts, &resolved) else {
        tracing::warn!(
            target: "quilltap::mount_index",
            targetPath = %target_path,
            resolved = %resolved,
            "No mount contains the target; not checking target persistence"
        );
        return PersistentTargetCheck::Persistent;
    };
    let ephemeral =
        mount.mount_point == "/" || EPHEMERAL_FS_TYPES.contains(&mount.fs_type.as_str());
    tracing::debug!(
        target: "quilltap::mount_index",
        targetPath = %target_path,
        resolved = %resolved,
        mountPoint = %mount.mount_point,
        fsType = %mount.fs_type,
        ephemeral,
        "Checked target persistence"
    );
    if !ephemeral {
        return PersistentTargetCheck::Persistent;
    }
    PersistentTargetCheck::Ephemeral {
        mount_point: mount.mount_point.clone(),
        fs_type: mount.fs_type.clone(),
        message: ephemeral_target_message(target_path),
    }
}

/// v4 `assertPersistentTarget(targetPath)` — the deconvert guard (no reachable
/// production caller in v5; module docs).
pub fn assert_persistent_target(
    target_path: &str,
    is_docker: bool,
    source: &dyn MountInfoSource,
) -> Result<(), EphemeralTargetError> {
    match check_persistent_target(target_path, is_docker, source) {
        PersistentTargetCheck::Persistent => Ok(()),
        PersistentTargetCheck::Ephemeral {
            mount_point,
            fs_type,
            message,
        } => {
            tracing::warn!(
                target: "quilltap::mount_index",
                targetPath = %target_path,
                mountPoint = %mount_point,
                fsType = %fs_type,
                "Refusing write to a path inside the container layer"
            );
            Err(EphemeralTargetError {
                target_path: target_path.to_string(),
                message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    //! v4 `lib/mount-index/__tests__/persistent-target.test.ts`'s rows (its
    //! `DOCKER_DESKTOP_MOUNTINFO` text byte-for-byte) + the capture pins for
    //! the four lines, each with its silence leg.
    use super::*;
    use crate::test_support::global_capture;

    /// Trimmed from a real Docker Desktop (macOS) container.
    const DOCKER_DESKTOP_MOUNTINFO: &str = concat!(
        "134 94 0:53 / / rw,relatime - overlay overlay rw,lowerdir=/x,upperdir=/y,workdir=/z\n",
        "136 134 0:62 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw\n",
        "137 134 0:63 / /dev rw,nosuid - tmpfs tmpfs rw,size=65536k,mode=755\n",
        "142 137 0:66 / /dev/shm rw,nosuid,nodev,noexec,relatime - tmpfs shm rw,size=65536k\n",
        "143 134 0:43 /csebold/Library/Mobile\\040Documents/Quilltap/V4test /app/quilltap rw,nosuid,nodev,relatime - virtiofs virtiofs0 rw\n",
        "150 134 0:43 /csebold/Vaults/My\\040Notes /Users/csebold/Vaults/My\\040Notes rw,relatime - virtiofs virtiofs0 rw\n",
        "144 134 254:1 /docker/containers/abc/hostname /etc/hostname rw,relatime - ext4 /dev/vda1 rw,discard\n",
        "160 134 0:70 / /scratch rw,relatime - tmpfs tmpfs rw\n",
        "170 134 254:1 /docker/srv/qtvol/_data /srv/qtvol rw,relatime shared:1 - ext4 /dev/vda1 rw\n",
    );

    struct Text(&'static str);
    impl MountInfoSource for Text {
        fn read_mount_info(&self) -> std::io::Result<String> {
            Ok(self.0.to_string())
        }
    }
    struct Unreadable;
    impl MountInfoSource for Unreadable {
        fn read_mount_info(&self) -> std::io::Result<String> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "EACCES",
            ))
        }
    }
    /// Panics if read — the not-docker gate must return first.
    struct NeverRead;
    impl MountInfoSource for NeverRead {
        fn read_mount_info(&self) -> std::io::Result<String> {
            panic!("the mount table must not be read outside a container")
        }
    }

    fn entry(mp: &str, fs: &str) -> MountEntry {
        MountEntry {
            mount_point: mp.to_string(),
            fs_type: fs.to_string(),
        }
    }

    #[test]
    fn reads_mount_points_and_filesystem_types_unescaping_spaces() {
        let mounts = parse_mount_info(DOCKER_DESKTOP_MOUNTINFO);
        assert!(mounts.contains(&entry("/", "overlay")));
        assert!(mounts.contains(&entry("/app/quilltap", "virtiofs")));
        assert!(mounts.contains(&entry("/Users/csebold/Vaults/My Notes", "virtiofs")));
    }

    #[test]
    fn finds_the_filesystem_type_after_optional_fields() {
        assert!(parse_mount_info(DOCKER_DESKTOP_MOUNTINFO).contains(&entry("/srv/qtvol", "ext4")));
    }

    #[test]
    fn skips_malformed_lines() {
        assert!(parse_mount_info("garbage\n\n1 2 3").is_empty());
    }

    #[test]
    fn picks_the_longest_containing_mount_point() {
        let mounts = parse_mount_info(DOCKER_DESKTOP_MOUNTINFO);
        let m = |p: &str| find_containing_mount(&mounts, p).map(|m| m.mount_point.clone());
        assert_eq!(
            m("/app/quilltap/stores/x").as_deref(),
            Some("/app/quilltap")
        );
        // A sibling with a shared prefix is not contained.
        assert_eq!(m("/app/quilltap-other/x").as_deref(), Some("/"));
        // The mount point itself matches.
        assert_eq!(m("/app/quilltap").as_deref(), Some("/app/quilltap"));
    }

    #[test]
    fn a_tie_goes_to_the_last_mount() {
        let mounts = vec![entry("/data", "ext4"), entry("/data", "tmpfs")];
        assert_eq!(
            find_containing_mount(&mounts, "/data/x").unwrap().fs_type,
            "tmpfs"
        );
    }

    fn check(target: &str) -> PersistentTargetCheck {
        check_persistent_target(target, true, &Text(DOCKER_DESKTOP_MOUNTINFO))
    }

    #[test]
    fn refuses_a_path_in_the_container_root_overlay() {
        match check("/tmp/qt-deconvert-test") {
            PersistentTargetCheck::Ephemeral {
                mount_point,
                message,
                ..
            } => {
                assert_eq!(mount_point, "/");
                assert!(message.contains("lost when the container is recreated"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn refuses_a_path_on_a_tmpfs() {
        assert!(matches!(
            check("/scratch/out"),
            PersistentTargetCheck::Ephemeral { fs_type, .. } if fs_type == "tmpfs"
        ));
    }

    #[test]
    fn accepts_binds_and_named_volumes() {
        assert_eq!(
            check("/app/quilltap/exports/store"),
            PersistentTargetCheck::Persistent
        );
        assert_eq!(
            check("/Users/csebold/Vaults/My Notes/out"),
            PersistentTargetCheck::Persistent
        );
        assert_eq!(check("/srv/qtvol/store"), PersistentTargetCheck::Persistent);
    }

    #[test]
    fn is_a_no_op_outside_a_container_and_silent() {
        global_capture::install();
        let (got, lines) =
            global_capture::capture(|| check_persistent_target("/tmp/anything", false, &NeverRead));
        assert_eq!(got, PersistentTargetCheck::Persistent);
        assert!(lines.is_empty(), "{lines:?}");
    }

    #[test]
    fn fails_open_when_the_mount_table_cannot_be_read() {
        global_capture::install();
        let (got, lines) =
            global_capture::capture(|| check_persistent_target("/tmp/x", true, &Unreadable));
        assert_eq!(got, PersistentTargetCheck::Persistent);
        assert_eq!(
            lines,
            vec![
                "WARN quilltap::mount_index Could not read the mount table; not checking target \
                 persistence targetPath=/tmp/x error=EACCES"
                    .to_string()
            ]
        );
    }

    #[test]
    fn no_containing_mount_warns_and_fails_open() {
        global_capture::install();
        let (got, lines) = global_capture::capture(|| {
            check_persistent_target(
                "/vol/x",
                true,
                &Text("11 1 0:10 / /other rw - xfs /dev/x rw"),
            )
        });
        assert_eq!(got, PersistentTargetCheck::Persistent);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(
            "WARN quilltap::mount_index No mount contains the target; not checking target \
             persistence targetPath=/vol/x resolved="
        ));
    }

    #[test]
    fn every_check_logs_its_debug_line_before_answering() {
        global_capture::install();
        let (_, lines) = global_capture::capture(|| check("/srv/qtvol/store"));
        assert_eq!(
            lines,
            vec![
                "DEBUG quilltap::mount_index Checked target persistence targetPath=/srv/qtvol/store \
                 resolved=/srv/qtvol/store mountPoint=/srv/qtvol fsType=ext4 ephemeral=false"
                    .to_string()
            ]
        );
    }

    #[test]
    fn judges_a_symlinked_directory_by_where_it_points() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = std::fs::canonicalize(dir.path()).unwrap();
        let target = real_dir.join("real");
        std::fs::create_dir(&target).unwrap();
        let link = real_dir.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let text = format!(
            "1 0 0:1 / / rw - overlay overlay rw\n2 1 0:2 / {} rw - virtiofs virtiofs0 rw",
            link.display()
        );
        struct Owned(String);
        impl MountInfoSource for Owned {
            fn read_mount_info(&self) -> std::io::Result<String> {
                Ok(self.0.clone())
            }
        }
        let got = check_persistent_target(
            &link.join("new-store").to_string_lossy(),
            true,
            &Owned(text),
        );
        assert!(
            matches!(got, PersistentTargetCheck::Ephemeral { .. }),
            "{got:?}"
        );
    }

    /// The deconvert guard's WARN (live only through the future deconvert
    /// order) + its silence on a persistent target.
    #[test]
    fn assert_persistent_target_warns_and_refuses() {
        global_capture::install();
        let (got, lines) = global_capture::capture(|| {
            assert_persistent_target("/scratch/out", true, &Text(DOCKER_DESKTOP_MOUNTINFO))
        });
        let err = got.unwrap_err();
        assert_eq!(err.target_path, "/scratch/out");
        assert_eq!(err.message, ephemeral_target_message("/scratch/out"));
        assert_eq!(
            lines.last().map(String::as_str),
            Some(
                "WARN quilltap::mount_index Refusing write to a path inside the container layer \
                 targetPath=/scratch/out mountPoint=/scratch fsType=tmpfs"
            )
        );
        let (ok, lines) = global_capture::capture(|| {
            assert_persistent_target("/srv/qtvol/store", true, &Text(DOCKER_DESKTOP_MOUNTINFO))
        });
        assert!(ok.is_ok());
        assert!(!lines.iter().any(|l| l.starts_with("WARN")), "{lines:?}");
    }
}
