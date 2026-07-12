//! Port of v4 `lib/mount-index/file-ops.ts` — mount-point file operations
//! (move / copy / link across mounts).
//!
//! P4.6v unit 3 lands only [`resolve_fs_absolute`] — the security-load-bearing
//! boundary-escape guard shared with the read path. The four cross-mount
//! strategies (`db-link` / `fs-link` / `rename` / `byte-copy`) land in a later
//! unit.

use super::file_op_error::{FileOpError, FileOpErrorCode};
use super::path_utils::path_posix_resolve;

/// v4 `resolveFsAbsolute`: resolve a filesystem-mount relative path to an
/// absolute path, refusing any result that escapes the mount's `basePath`.
///
/// The guard mirrors v4 exactly: `abs !== base && !abs.startsWith(base + sep)`.
/// `base_path` is `None` when the mount has no `basePath` configured (v4's
/// `if (!mp.basePath)` falsy check).
pub fn resolve_fs_absolute(
    base_path: Option<&str>,
    mount_id: &str,
    relative_path: &str,
) -> Result<String, FileOpError> {
    let base_path = match base_path {
        Some(b) if !b.is_empty() => b,
        _ => {
            return Err(FileOpError::new(
                format!("Filesystem mount has no basePath configured: {mount_id}"),
                FileOpErrorCode::InvalidPath,
            ))
        }
    };
    let abs = path_posix_resolve(&[base_path, relative_path]);
    let base = path_posix_resolve(&[base_path]);
    let base_with_sep = if base.ends_with('/') {
        base.clone()
    } else {
        format!("{base}/")
    };
    if abs != base && !abs.starts_with(&base_with_sep) {
        return Err(FileOpError::new(
            format!("Path escapes mount boundary: {relative_path}"),
            FileOpErrorCode::InvalidPath,
        ));
    }
    Ok(abs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_fs_absolute_pins_the_boundary_guard() {
        let base = Some("/mnt/vault");
        // In-bounds paths resolve.
        assert_eq!(
            resolve_fs_absolute(base, "m1", "notes/a.md").unwrap(),
            "/mnt/vault/notes/a.md"
        );
        // The base itself resolves.
        assert_eq!(resolve_fs_absolute(base, "m1", "").unwrap(), "/mnt/vault");
        // `..` that resolves within bounds is allowed (path.resolve collapses it).
        assert_eq!(
            resolve_fs_absolute(base, "m1", "notes/../a.md").unwrap(),
            "/mnt/vault/a.md"
        );
        // Escapes are refused with INVALID_PATH.
        for escape in ["../secret", "../../etc/passwd", "notes/../../escape"] {
            let e = resolve_fs_absolute(base, "m1", escape).unwrap_err();
            assert_eq!(e.code, FileOpErrorCode::InvalidPath, "escape {escape}");
        }
        // A sibling directory sharing a prefix does NOT count as in-bounds.
        let e = resolve_fs_absolute(Some("/mnt/vault"), "m1", "../vault-evil/x").unwrap_err();
        assert_eq!(e.code, FileOpErrorCode::InvalidPath);
        // No basePath configured.
        let e = resolve_fs_absolute(None, "m1", "a.md").unwrap_err();
        assert_eq!(e.code, FileOpErrorCode::InvalidPath);
        let e = resolve_fs_absolute(Some(""), "m1", "a.md").unwrap_err();
        assert_eq!(e.code, FileOpErrorCode::InvalidPath);
    }
}
