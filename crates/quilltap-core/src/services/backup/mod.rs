//! The backup family (P4.9G5) — v4 `lib/backup/**`.
//!
//! Landed: the read-and-project half ([`collect_user_data`],
//! [`create_manifest`], [`stage_backup`]), the archive ([`archive`]), and the
//! `createBackup` orchestration over the [`BackupHost`] seam. The restore side
//! (upload / preview / execute) is NOT ported — see the order's status header
//! and the lane record for the resume list; its verbs answer the loud
//! not-yet-available refusal.

pub mod archive;
pub mod collect;
pub mod manifest;
mod marshal;
pub mod staging;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

pub use collect::{collect_user_data, BackupData};
pub use manifest::{create_manifest, HostCounts};
pub use staging::{count_subdirs, stage_backup, HostDirs, StageReport};

use crate::db::runtime::Db;
use crate::services::file_storage::StorageBackend;

/// Everything the backup family needs from the host process (P4.0: the host
/// owns cadence, the filesystem, and process-lifetime state; the core owns the
/// projection).
///
/// A `None` seam on `EngineAssembly` → the `systemBackupCreate` arm answers the
/// loud not-assembled refusal, exactly like the other host seams.
pub trait BackupHost: Send + Sync {
    /// The disk backend user-file bytes are staged from (v4
    /// `fileStorageManager` over `<base>/files`).
    fn storage(&self) -> Arc<dyn StorageBackend>;
    /// The scratch root new staging directories are made under (v4
    /// `os.tmpdir()`).
    fn temp_dir(&self) -> PathBuf;
    /// The host directories v4 copies wholesale into the archive.
    fn host_dirs(&self) -> HostDirs;
    /// v5's build version — the analogue of v4's
    /// `process.env.npm_package_version || '2.0.0'` (`backup-service.ts:44`).
    fn app_version(&self) -> String;
    /// Wall clock, milliseconds since the epoch. **Injected** so the temp
    /// store's TTL sweep is testable.
    fn now_ms(&self) -> i64;
    /// v4 `storeTemporaryBackup` (`temporary-storage.ts:92`) — remember
    /// `backupId → zipPath` for the 30-minute download window.
    fn store_backup(&self, backup_id: &str, zip_path: &Path);
    /// v4 `retrieveTemporaryBackup` (`:115`) — **single-use**: the entry is
    /// removed on read, so a backup can only be downloaded once.
    fn take_backup(&self, backup_id: &str) -> Option<PathBuf>;
}

/// v4 `createBackup`'s return (`backup-service.ts:548`).
pub struct CreatedBackup {
    pub zip_path: PathBuf,
    pub manifest: Value,
    pub staged: StageReport,
}

/// v4 `createBackup(userId)` (`:548`) — collect, stage, zip, and hand back the
/// archive path plus the manifest. The caller mints the backup id and registers
/// it in the temp store (v4 does that in the route, `system/backup/route.ts:31`).
///
/// v4's first step is a WAL checkpoint of all three partitions (`:556-570`); v5
/// opens `journal_mode = TRUNCATE` (the standing cloud-sync rule), so there is
/// no WAL to flush and the read is already consistent.
///
/// On any failure the whole temp directory is removed (v4's `catch` at `:760`).
pub fn create_backup(
    db: &Db,
    host: &dyn BackupHost,
    user_id: &str,
    created_at_iso: &str,
) -> Result<CreatedBackup, String> {
    let data = collect_user_data(db, user_id).map_err(|e| e.to_string())?;
    let dirs = host.host_dirs();
    let counts = HostCounts {
        npm_plugins: count_subdirs(dirs.npm_plugins.as_deref(), &[]),
        user_installed_themes: count_subdirs(dirs.themes.as_deref(), &[".cache"]),
    };
    let manifest = create_manifest(user_id, &data, &host.app_version(), created_at_iso, counts);

    // v4 `mkdtemp(path.join(os.tmpdir(), 'quilltap-backup-'))` (`:577`), then a
    // staging folder named `quilltap-backup-<ISO with [:.] → ->` inside it.
    let temp_dir = host
        .temp_dir()
        .join(format!("quilltap-backup-{}", uuid::Uuid::new_v4()));
    let folder_name = format!("quilltap-backup-{}", sanitize_timestamp(created_at_iso));
    let staging_dir = temp_dir.join(&folder_name);
    std::fs::create_dir_all(&staging_dir).map_err(|e| e.to_string())?;

    let result = (|| -> Result<CreatedBackup, String> {
        let staged = stage_backup(
            db,
            host.storage().as_ref(),
            &data,
            &manifest,
            &staging_dir,
            &dirs,
        )
        .map_err(|e| e.to_string())?;
        let zip_path = temp_dir.join(format!("{folder_name}.zip"));
        archive::zip_staging_dir(&staging_dir, &folder_name, &zip_path)
            .map_err(|e| e.to_string())?;
        // v4 removes the staging folder and keeps only the zip (`:747`).
        let _ = std::fs::remove_dir_all(&staging_dir);
        Ok(CreatedBackup {
            zip_path,
            manifest: manifest.clone(),
            staged,
        })
    })();

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    result
}

/// v4 `new Date().toISOString().replace(/[:.]/g, '-')` (`:578`).
fn sanitize_timestamp(iso: &str) -> String {
    iso.chars()
        .map(|c| if c == ':' || c == '.' { '-' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn timestamp_sanitization_matches_v4() {
        assert_eq!(
            super::sanitize_timestamp("2026-07-24T18:14:39.863Z"),
            "2026-07-24T18-14-39-863Z"
        );
    }
}
