//! The host half of the backup family (P4.9G5) — the [`BackupHost`] seam.
//!
//! v4 keeps the pending-download map in a module-level `Map` guarded by
//! `globalThis` (`lib/backup/temporary-storage.ts`) with a one-minute
//! `setInterval` sweep. In v5 that is host state by construction (P4.0: the
//! host owns cadence and process-lifetime state), so it lives here behind a
//! `Mutex<HashMap>` — and the sweep is **lazy** rather than a timer: every
//! `store`/`take` first drops the expired entries and unlinks their zips. Same
//! observable behavior (an entry older than 30 minutes is gone and its file is
//! deleted), one less thread, and it is testable because the clock is injected.
//!
//! The retrieval is **single-use**, exactly as v4's `retrieveTemporaryBackup`
//! removes the entry on read: a backup can be downloaded once.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use quilltap_core::services::backup::{BackupHost, HostDirs};
use quilltap_core::services::file_storage::StorageBackend;

use crate::files_store::LocalStorageBackend;

/// v4 `BACKUP_EXPIRY_MS` (`temporary-storage.ts:29`).
const BACKUP_EXPIRY_MS: i64 = 30 * 60 * 1000;

/// The wall clock, injected so the TTL is testable.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// The production clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

struct PendingBackup {
    zip_path: PathBuf,
    created_at_ms: i64,
}

/// The live [`BackupHost`].
pub struct HostBackupServices {
    base_dir: PathBuf,
    app_version: String,
    clock: Arc<dyn Clock>,
    storage: Arc<dyn StorageBackend>,
    pending: Mutex<HashMap<String, PendingBackup>>,
}

impl HostBackupServices {
    pub fn new(base_dir: PathBuf, app_version: String, clock: Arc<dyn Clock>) -> Self {
        let storage: Arc<dyn StorageBackend> =
            Arc::new(LocalStorageBackend::new(base_dir.join("files")));
        HostBackupServices {
            base_dir,
            app_version,
            clock,
            storage,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// v4's `CLEANUP_INTERVAL_MS` sweep body, run lazily: drop every entry past
    /// its 30-minute expiry and unlink its zip (plus the temp directory the zip
    /// lives in, which is this backup's alone).
    fn sweep(&self, pending: &mut HashMap<String, PendingBackup>) {
        let now = self.clock.now_ms();
        let expired: Vec<String> = pending
            .iter()
            .filter(|(_, b)| now - b.created_at_ms > BACKUP_EXPIRY_MS)
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            if let Some(b) = pending.remove(&id) {
                remove_zip_and_dir(&b.zip_path);
            }
        }
    }
}

/// Delete the archive and the private temp directory holding it (v4's download
/// handler does exactly this on stream close, `system/backup/[id]/route.ts:63`).
pub fn remove_zip_and_dir(zip_path: &Path) {
    let _ = std::fs::remove_file(zip_path);
    if let Some(dir) = zip_path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

impl BackupHost for HostBackupServices {
    fn storage(&self) -> Arc<dyn StorageBackend> {
        self.storage.clone()
    }

    fn temp_dir(&self) -> PathBuf {
        std::env::temp_dir()
    }

    fn host_dirs(&self) -> HostDirs {
        HostDirs {
            npm_plugins: Some(self.base_dir.join("plugins").join("npm")),
            themes: Some(self.base_dir.join("themes")),
        }
    }

    fn app_version(&self) -> String {
        self.app_version.clone()
    }

    fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }

    fn store_backup(&self, backup_id: &str, zip_path: &Path) {
        let mut pending = self.pending.lock().unwrap();
        self.sweep(&mut pending);
        pending.insert(
            backup_id.to_string(),
            PendingBackup {
                zip_path: zip_path.to_path_buf(),
                created_at_ms: self.clock.now_ms(),
            },
        );
    }

    fn take_backup(&self, backup_id: &str) -> Option<PathBuf> {
        let mut pending = self.pending.lock().unwrap();
        self.sweep(&mut pending);
        pending.remove(backup_id).map(|b| b.zip_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    struct FixedClock(AtomicI64);
    impl Clock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    fn services(clock: Arc<FixedClock>) -> (HostBackupServices, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (
            HostBackupServices::new(dir.path().to_path_buf(), "0.0.0".into(), clock),
            dir,
        )
    }

    #[test]
    fn retrieval_is_single_use() {
        let clock = Arc::new(FixedClock(AtomicI64::new(1_000)));
        let (svc, dir) = services(clock);
        let zip = dir.path().join("a.zip");
        std::fs::write(&zip, b"z").unwrap();
        svc.store_backup("id-1", &zip);
        assert_eq!(svc.take_backup("id-1"), Some(zip));
        assert_eq!(svc.take_backup("id-1"), None);
    }

    #[test]
    fn an_entry_past_the_thirty_minute_expiry_is_swept_and_its_zip_unlinked() {
        let clock = Arc::new(FixedClock(AtomicI64::new(0)));
        let (svc, dir) = services(clock.clone());
        // The zip lives in its own temp directory, as `create_backup` arranges.
        let zip_dir = dir.path().join("backup-tmp");
        std::fs::create_dir_all(&zip_dir).unwrap();
        let zip = zip_dir.join("a.zip");
        std::fs::write(&zip, b"z").unwrap();
        svc.store_backup("id-1", &zip);

        clock.0.store(BACKUP_EXPIRY_MS, Ordering::SeqCst);
        assert!(
            svc.take_backup("id-1").is_some(),
            "exactly at the TTL: kept"
        );

        svc.store_backup("id-2", &zip);
        clock.0.store(2 * BACKUP_EXPIRY_MS + 1, Ordering::SeqCst);
        assert_eq!(svc.take_backup("id-2"), None, "past the TTL: swept");
        assert!(!zip.exists(), "the expired archive is deleted from disk");
    }
}
