//! The production filesystem half of [`quilltap_core::write_apply::ApplyHost`]
//! (P4.1b) — the four fs methods a full `ApplyHost` composition delegates to.
//!
//! Inventory completion only: since U4.4 moved the enclave turn to direct
//! writes, no production consumer drives `write_apply` yet — a future
//! batch-mode job composes a full `ApplyHost` (connections + repo dispatch)
//! around these ops. Signatures mirror the trait's fs methods one-for-one so
//! that composition is a delegation.

use std::path::Path;

use quilltap_core::write_apply::ApplyError;

/// The four `ApplyHost` fs operations (v4 `applyWritesUnsafe`'s
/// `__finalizeFile` + post-commit side effects).
#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyFsOps;

impl ApplyFsOps {
    /// `__finalizeFile`: ensure `final_dir` exists, then rename
    /// `staging_path` → `final_path` (v4 `ensureDirSync` + `fs.renameSync`).
    /// An `Err` triggers the caller's rollback + undo of earlier finalizes.
    pub fn finalize_file(
        &self,
        final_dir: &str,
        staging_path: &str,
        final_path: &str,
    ) -> Result<(), ApplyError> {
        std::fs::create_dir_all(final_dir)
            .map_err(|e| ApplyError::msg(format!("ensureDir failed for {final_dir}: {e}")))?;
        std::fs::rename(staging_path, final_path).map_err(|e| {
            ApplyError::msg(format!("rename failed {staging_path} -> {final_path}: {e}"))
        })
    }

    /// Undo a completed finalize on rollback: rename `final_path` →
    /// `staging_path`. Best-effort (v4 swallows the error).
    pub fn undo_finalize(&self, final_path: &str, staging_path: &str) {
        let _ = std::fs::rename(final_path, staging_path);
    }

    /// Post-commit: remove the per-job staging root (v4
    /// `fs.rmSync(root, {recursive: true, force: true})`). Best-effort.
    pub fn cleanup_staging_dir(&self, staging_root: &str) {
        let path = Path::new(staging_root);
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }
    }

    /// Post-commit cache invalidations. Deliberately a no-op in v5: there is
    /// no process-global mount cache (v4's `notifyChild` fed the forked child's
    /// caches, which do not exist under the in-process runtime) and the vector
    /// store loads per-use off the read pool — nothing to evict.
    pub fn dispatch_invalidations(
        &self,
        _vector_store_keys: &[String],
        _mount_point_keys: &[String],
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_creates_dir_and_renames() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("staging/file.bin");
        std::fs::create_dir_all(staging.parent().unwrap()).unwrap();
        std::fs::write(&staging, b"payload").unwrap();
        let final_dir = dir.path().join("final/nested");
        let final_path = final_dir.join("file.bin");

        let ops = ApplyFsOps;
        ops.finalize_file(
            final_dir.to_str().unwrap(),
            staging.to_str().unwrap(),
            final_path.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(std::fs::read(&final_path).unwrap(), b"payload");
        assert!(!staging.exists());

        // Undo puts it back; best-effort (a second undo is a no-op).
        ops.undo_finalize(final_path.to_str().unwrap(), staging.to_str().unwrap());
        assert!(staging.exists());
        assert!(!final_path.exists());
        ops.undo_finalize(final_path.to_str().unwrap(), staging.to_str().unwrap());
    }

    #[test]
    fn finalize_missing_staging_errors() {
        let dir = tempfile::tempdir().unwrap();
        let ops = ApplyFsOps;
        let err = ops
            .finalize_file(
                dir.path().to_str().unwrap(),
                dir.path().join("nope.bin").to_str().unwrap(),
                dir.path().join("out.bin").to_str().unwrap(),
            )
            .unwrap_err();
        assert!(err.message.contains("rename failed"));
    }

    #[test]
    fn cleanup_staging_is_recursive_and_tolerant() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join(".staging/job-1");
        std::fs::create_dir_all(root.join("deep/deeper")).unwrap();
        std::fs::write(root.join("deep/deeper/x"), b"x").unwrap();
        let ops = ApplyFsOps;
        ops.cleanup_staging_dir(root.to_str().unwrap());
        assert!(!root.exists());
        // Missing root is fine.
        ops.cleanup_staging_dir(root.to_str().unwrap());
        // Invalidations are a documented no-op.
        ops.dispatch_invalidations(&["v1".into()], &["m1".into()]);
    }
}
