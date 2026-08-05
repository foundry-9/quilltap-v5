//! The Almanack — Phase 1, "Taking the measure of the premises"
//! (v4 `lib/tools/almanack/phase1-premises.ts`).
//!
//! The building itself: the runtime it is standing in, the three databases and
//! their footprints, what backups exist, and how far the migration ladder has
//! been climbed.
//!
//! ## ⚠ Divergence — `runtimeEnvironment` describes v5's runtime
//!
//! v4's block reports `process.version`, `os.type()`, `process.uptime()` and so
//! on. The section's *purpose* is to describe the process that produced the
//! report, so a Rust host truthfully reports Rust facts: the labels are
//! byte-identical, the values are v5's. This is the same principle the port
//! applies to `version`. Both the whole block and `version` are normalized in
//! the tier-2 differential (they are wall-clock/machine facts, like
//! `generatedAt`), so nothing about them is asserted equal to v4's.
//!
//! `runtimeType` keeps v4's vocabulary: `docker` when the host says so, else
//! `node` — v5 has no `lima`/`electron` shell detection, and inventing values
//! outside v4's union would break the card that reads them.

use std::path::{Path, PathBuf};

use crate::db::runtime::Db;

use super::db::{main_row, main_rows, opt_text, text};
use super::types::{
    BackupInfo, DatabaseSecurityInfo, DatabaseSizeInfo, MigrationStateInfo, RuntimeEnvironmentInfo,
};

/// The on-disk facts the core cannot discover for itself — supplied by the
/// composition root (v4 reads them from `lib/paths`). Keeping them as an input
/// is what lets the differential pin every path.
#[derive(Debug, Clone)]
pub struct AlmanackPaths {
    /// `quilltap.db`.
    pub main_db: PathBuf,
    /// `quilltap-llm-logs.db`.
    pub llm_logs_db: Option<PathBuf>,
    /// `quilltap-mount-index.db`.
    pub mount_index_db: Option<PathBuf>,
    /// The instance data directory (v4 `getDataDir()`).
    pub data_dir: PathBuf,
    /// The physical-backup directory (v4 `getBackupsDir()`).
    pub backups_dir: PathBuf,
}

/// Host-supplied runtime facts (v4 reads `process`/`os`; v5's host owns them).
#[derive(Debug, Clone)]
pub struct RuntimeFacts {
    pub node_version: String,
    pub platform: String,
    pub arch: String,
    pub os_type: String,
    pub os_release: String,
    pub total_memory_bytes: f64,
    pub free_memory_bytes: f64,
    pub runtime_type: String,
    pub uptime_seconds: f64,
    /// v4 `Intl.DateTimeFormat().resolvedOptions().timeZone`.
    pub timezone: String,
}

/// v4 `collectRuntimeEnvironment` — see the module header's divergence note.
/// `electronShellVersion` is always absent and `shellCapabilities` always empty
/// in v5 (there is no Electron shell); both render as omitted lines, exactly as
/// they do on a v4 server install.
pub fn collect_runtime_environment(
    facts: &RuntimeFacts,
    paths: &AlmanackPaths,
) -> RuntimeEnvironmentInfo {
    RuntimeEnvironmentInfo {
        node_version: facts.node_version.clone(),
        platform: facts.platform.clone(),
        arch: facts.arch.clone(),
        os_type: facts.os_type.clone(),
        os_release: facts.os_release.clone(),
        total_memory_bytes: facts.total_memory_bytes,
        free_memory_bytes: facts.free_memory_bytes,
        runtime_type: facts.runtime_type.clone(),
        electron_shell_version: None,
        shell_capabilities: Vec::new(),
        uptime_seconds: facts.uptime_seconds,
        data_directory: paths.data_dir.to_string_lossy().into_owned(),
        timezone: facts.timezone.clone(),
    }
}

/// v4's `statSize` — the file may legitimately not exist yet (fresh install, or
/// a database whose first access hasn't happened).
fn stat_size(label: &str, path: Option<&Path>) -> DatabaseSizeInfo {
    let size = path
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| m.len());
    match size {
        Some(bytes) => DatabaseSizeInfo {
            label: label.to_string(),
            size_bytes: bytes as f64,
            present: true,
        },
        None => DatabaseSizeInfo {
            label: label.to_string(),
            size_bytes: 0.0,
            present: false,
        },
    }
}

/// v4 `collectDatabaseSecurity`. Three databases since 4.9 — the mount index is
/// where character content, wardrobe, photos, mail, scenarios and every
/// document byte actually live, so a report that only weighed the main DB was
/// weighing the wrong shelf.
///
/// `passphrase_protected` is host knowledge (v4 `getHasUserPassphrase()`), so it
/// arrives as a parameter rather than being guessed here.
pub fn collect_database_security(
    db: &Db,
    paths: &AlmanackPaths,
    passphrase_protected: bool,
) -> DatabaseSecurityInfo {
    let databases = vec![
        stat_size("Main", Some(&paths.main_db)),
        stat_size("LLM Logs", paths.llm_logs_db.as_deref()),
        stat_size("Mount Index", paths.mount_index_db.as_deref()),
    ];

    // The version guard's tripwire, stored in instance_settings. v4's old
    // report declared this field and then always rendered null.
    let highest_app_version = main_row(
        db,
        r#"SELECT "value" FROM "instance_settings" WHERE "key" = 'highest_app_version'"#,
        &[],
        "premises.highestAppVersion",
        |r| opt_text(r, 0),
    )
    .flatten();

    DatabaseSecurityInfo {
        passphrase_protected,
        databases,
        highest_app_version,
    }
}

/// One backup-filename parser: `quilltap[-<suffix>]-YYYY-MM-DDTHHmmss.db`.
/// Returns epoch ms. v4 builds a LOCAL-time `Date` from the parts and later
/// `toISOString()`s it; under the pinned `TZ=UTC` those coincide, and v5
/// computes UTC (the same standing constraint `format_time` documents).
fn parse_backup_filename(filename: &str, prefix: &str) -> Option<i64> {
    let rest = filename.strip_prefix(prefix)?.strip_suffix(".db")?;
    // `YYYY-MM-DDTHHmmss`
    let bytes = rest.as_bytes();
    if bytes.len() != 17 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let digits = |a: usize, b: usize| -> Option<i64> {
        let s = rest.get(a..b)?;
        if !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        s.parse().ok()
    };
    let year = digits(0, 4)?;
    let month = digits(5, 7)?;
    let day = digits(8, 10)?;
    let hour = digits(11, 13)?;
    let minute = digits(13, 15)?;
    let second = digits(15, 17)?;
    // v4's `new Date(y, m-1, d, …)` ROLLS out-of-range parts rather than
    // rejecting them (the regex already bounds them to two digits), so the
    // civil-days helper's rolling arithmetic is the faithful match.
    let days = crate::clock::days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1000)
}

/// v4 `collectBackupStatus` — the three parsers, in v4's order.
///
/// Order matters: the main-DB parser is the loosest, so the two prefixed
/// parsers must get first refusal on every filename.
pub fn collect_backup_status(paths: &AlmanackPaths) -> Vec<BackupInfo> {
    const PARSERS: [(&str, &str); 3] = [
        ("LLM Logs", "quilltap-llm-logs-"),
        ("Mount Index", "quilltap-mount-index-"),
        ("Main", "quilltap-"),
    ];

    let mut buckets: Vec<(&str, Vec<i64>, f64)> = PARSERS
        .iter()
        .map(|(label, _)| (*label, Vec::new(), 0.0))
        .collect();

    if let Ok(entries) = std::fs::read_dir(&paths.backups_dir) {
        // v4 iterates `fs.readdir`'s order; the buckets are sorted below, and
        // the only order-sensitive figure (total bytes) is a sum, so the
        // directory order cannot reach the output. Sorted anyway so a partial
        // failure is reproducible.
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for filename in names {
            for (idx, (_, prefix)) in PARSERS.iter().enumerate() {
                let Some(ms) = parse_backup_filename(&filename, prefix) else {
                    continue;
                };
                // Skip files we can't stat; the count still reflects reality.
                let size = std::fs::metadata(paths.backups_dir.join(&filename))
                    .map(|m| m.len() as f64)
                    .unwrap_or(0.0);
                buckets[idx].1.push(ms);
                buckets[idx].2 += size;
                break;
            }
        }
    }

    // Present in main → llm-logs → mount-index order, matching the DB size table.
    ["Main", "LLM Logs", "Mount Index"]
        .iter()
        .map(|label| {
            let bucket = buckets.iter().find(|(l, _, _)| l == label).unwrap();
            let mut sorted = bucket.1.clone();
            sorted.sort_unstable();
            BackupInfo {
                label: (*label).to_string(),
                count: sorted.len() as f64,
                oldest_date: sorted.first().map(|ms| crate::clock::iso_from_unix_ms(*ms)),
                newest_date: sorted.last().map(|ms| crate::clock::iso_from_unix_ms(*ms)),
                total_size_bytes: bucket.2,
            }
        })
        .collect()
}

/// v4 `collectMigrationState` — how far up the ladder this database has climbed.
///
/// A v5-provisioned instance has no `migrations_state` table at all (the
/// migration runner stays deferred), so this takes the soft-failure arm and
/// reports the zeroed shape — which is also exactly what v4 reports for a
/// database that has never migrated. The differential records which arm each
/// fixture exercises.
pub fn collect_migration_state(db: &Db) -> MigrationStateInfo {
    let rows: Vec<(String, Option<String>, Option<String>)> = main_rows(
        db,
        r#"SELECT "id", "completedAt", "quilltapVersion"
     FROM "migrations_state"
     ORDER BY "completedAt" DESC"#,
        &[],
        "premises.migrationState",
        |r| Ok((text(r, 0)?, opt_text(r, 1)?, opt_text(r, 2)?)),
    );

    let latest = rows.first();
    MigrationStateInfo {
        applied_count: rows.len() as f64,
        last_migration_id: latest.map(|r| r.0.clone()),
        last_migration_at: latest.and_then(|r| r.1.clone()),
        last_migration_version: latest.and_then(|r| r.2.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_filenames_parse_like_v4() {
        // `quilltap-2026-08-04T031500.db` → 2026-08-04T03:15:00Z under TZ=UTC.
        let ms = parse_backup_filename("quilltap-2026-08-04T031500.db", "quilltap-").unwrap();
        assert_eq!(
            crate::clock::iso_from_unix_ms(ms),
            "2026-08-04T03:15:00.000Z"
        );
        // The prefixed parsers must not be shadowed by the loose main one — the
        // caller tries them first, and the main prefix DOES match these names.
        assert!(parse_backup_filename(
            "quilltap-llm-logs-2026-08-04T031500.db",
            "quilltap-llm-logs-"
        )
        .is_some());
        assert!(
            parse_backup_filename("quilltap-llm-logs-2026-08-04T031500.db", "quilltap-").is_none()
        );
        // Rejections.
        assert!(parse_backup_filename("quilltap-2026-08-04T0315.db", "quilltap-").is_none());
        assert!(parse_backup_filename("quilltap-backup.db", "quilltap-").is_none());
        assert!(parse_backup_filename("quilltap-2026-08-04T031500.sqlite", "quilltap-").is_none());
    }
}
