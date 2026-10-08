//! Daily database optimization (startup) — port of v4 `f5e953a3f`'s
//! `lib/startup/daily-db-optimize.ts` (P4.D259), v4's PHASE 0.75.
//!
//! v4's header, carried: once per local calendar day, before migrations or
//! anything else touches the data, each of the three SQLite databases gets the
//! same treatment the CLI's `quilltap db optimize` gives it
//! (`packages/quilltap/lib/db-commands.js`, `optimizeOneDb`): VACUUM, ANALYZE,
//! `PRAGMA optimize`. Keep the two step lists in step (v5: this module's
//! [`run_optimize_steps`] IS the CLI's — `quilltap-cli`'s `db optimize` calls
//! it).
//!
//! Each database is backed up first through the ordinary physical-backup
//! functions ([`super::physical_backup`]), which take a backup only when the
//! last one is more than 24 hours old — so the daily backup still happens at
//! most once a day, it simply lands before the optimize on the day's first
//! launch, and the startup backup (the host's PHASE-2 fire-and-forget) then
//! finds it and skips.
//!
//! The gate is a small JSON file in the data directory recording, per
//! database, the local date of the last successful optimize. A database whose
//! optimize fails is not stamped and is tried again on the next launch.
//! Nothing here is fatal: a failure is logged and startup carries on.
//!
//! **Where v5 runs it.** At the head of the host's `assemble`, before
//! `seed_built_ins` (v4: after the version guard, before the migrations), on
//! the WRITER connections: the steps run inside ONE `write_blocking` closure
//! per database, each in autocommit (`VACUUM` refuses inside a transaction).
//! The pre-optimize backup runs on the read pool (`VACUUM INTO` is legal on a
//! read-only connection). Every line is emitted on the CALLER thread from the
//! returned [`StepResult`]s — a line logged inside the writer closure is
//! invisible to the thread-scoped capture rig, and the order is v4's either
//! way.
//!
//! **Recorded divergences / NO-PORT (P4.D259 R-A, R-B, R-C):**
//! - **No `wal_checkpoint(TRUNCATE)`** (`WAL_CHECKPOINT_UNDER_TRUNCATE`): v4's
//!   `!spec.owned` arm folds the shared main handle's WAL back in after the
//!   VACUUM because v4's migration-layer openers set `journal_mode = WAL`; v5's
//!   writers are TRUNCATE (no WAL exists), so the step and its WARN `Post-
//!   optimize WAL checkpoint failed` have nothing to do. Nor is there an
//!   `Error closing database after optimize` analog — v5 owns no handle here.
//! - **No progress labels.** v5 has no startup-progress label stream, so v4's
//!   `setCurrent('subsystem:db-optimize:start')` (pretty: "Giving the ledgers
//!   their morning dusting"), the per-database `setSubProgress([{current, total,
//!   unit: 'databases'}])` and the closing `publish({rawLabel:
//!   'subsystem:db-optimize:complete', detail: '${optimized} of ${work.length}
//!   databases optimized in ${(elapsedMs / 1000).toFixed(1)} s'})` (pretty:
//!   "Ledgers dusted and squared away") have nowhere to land. v4's
//!   `!isSQLiteBackend()` DEBUG `Not a SQLite backend; nothing to optimize` is
//!   dead in v4 itself.
//! - **A DEGRADED sibling** (`DEGRADED_OPTIMIZE_ERROR_TEXT`): v4 re-opens the
//!   file through its migration-layer opener, which throws on the damaged
//!   header (`file is not a database`), and lands in the per-database ERROR arm
//!   — NOT stamped. v5 never re-opens (the partition opened degraded and has no
//!   writer), takes the same arm, and renders the partition's degraded
//!   sentence as the `error` VALUE: `Db` retains no open-time SQLite text.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::db::runtime::{Db, PartitionState, WriterSet};
use crate::db::table_shape::Partition;
use crate::db::DbError;
use crate::host_zone::TimeZone;
use crate::services::physical_backup::{self, node_fs_message, BackupKind};

/// v4's child-logger module for this file.
pub const DAILY_DB_OPTIMIZE_MODULE: &str = "startup:daily-db-optimize";

/// The state keys AND the pass labels, in v4's order. The mount index's key is
/// `mount-points` (its backup SET label is `mount-index`).
pub const OPTIMIZE_TARGET_KEYS: [&str; 3] = ["main", "llm-logs", "mount-points"];

/// Name of the gate file, under the data directory.
pub const OPTIMIZE_STATE_FILENAME: &str = "db-optimize-state.json";

/// The literal `backupPath` v4 logs when the backup function answered `null`.
pub const BACKUP_SKIPPED_TEXT: &str =
    "(skipped — a recent backup exists, or the backup failed; see above)";

/// `path.join(getDataDir(), OPTIMIZE_STATE_FILENAME)`.
pub fn optimize_state_path(data_dir: &Path) -> PathBuf {
    data_dir.join(OPTIMIZE_STATE_FILENAME)
}

/// v4 `localDateStamp` — the local calendar date (`zone`) as `YYYY-MM-DD`.
pub fn local_date_stamp(now_ms: i64, zone: &TimeZone) -> String {
    let (y, m, d, ..) = physical_backup::local_parts(now_ms, zone);
    format!("{y}-{m:02}-{d:02}")
}

/// Node's `fs.readFileSync(path, 'utf8')`: the open's failure names the path,
/// the read's does not (`EISDIR: illegal operation on a directory, read` — a
/// directory opens fine and fails at the read); invalid UTF-8 decodes to
/// U+FFFD.
fn read_file_sync_utf8(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file =
        std::fs::File::open(path).map_err(|e| node_fs_message(&e, "open", Some(path)))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| node_fs_message(&e, "read", None))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// v4 `readOptimizeState` — a missing file, a non-object, an array or `null`
/// read as "never optimized" SILENTLY; a read or parse failure does too, with
/// the WARN. Keeps only v4's three keys, and only string values, in v4's key
/// order (unknown keys are dropped here, so never written back).
pub fn read_optimize_state(state_path: &Path) -> Map<String, Value> {
    let warn = |error: &str| {
        tracing::warn!(
            target: "quilltap::startup",
            module = DAILY_DB_OPTIMIZE_MODULE,
            statePath = state_path.display().to_string().as_str(),
            error,
            "Could not read database optimize state; treating every database as due"
        );
        Map::new()
    };
    if std::fs::metadata(state_path).is_err() {
        return Map::new();
    }
    let text = match read_file_sync_utf8(state_path) {
        Ok(t) => t,
        Err(error) => return warn(&error),
    };
    let parsed: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        // V8's `JSON.parse` message; where V8 would ACCEPT what serde refuses
        // (a number past f64, a lone-surrogate escape) serde's message stands
        // — the twin's recorded divergence.
        Err(e) => {
            return warn(
                &crate::jsstr::v8_json_parse_message(&text).unwrap_or_else(|| e.to_string()),
            )
        }
    };
    let Value::Object(parsed) = parsed else {
        return Map::new();
    };
    let mut state = Map::new();
    for key in OPTIMIZE_TARGET_KEYS {
        if let Some(Value::String(s)) = parsed.get(key) {
            state.insert(key.to_string(), Value::String(s.clone()));
        }
    }
    state
}

/// v4 `writeOptimizeState` — `JSON.stringify(state, null, 2) + '\n'`, keys in
/// the map's (insertion) order; a failure is the WARN, never an error.
pub fn write_optimize_state(state: &Map<String, Value>, state_path: &Path) {
    let mut text = serde_json::to_string_pretty(&Value::Object(state.clone()))
        .expect("a map of strings serializes");
    text.push('\n');
    if let Err(e) = std::fs::write(state_path, text) {
        tracing::warn!(
            target: "quilltap::startup",
            module = DAILY_DB_OPTIMIZE_MODULE,
            statePath = state_path.display().to_string().as_str(),
            error = node_fs_message(&e, "open", Some(state_path)).as_str(),
            "Could not write database optimize state; optimize will repeat on next launch"
        );
    }
}

/// v4 `isOptimizeDue` — `state[key] !== today`.
pub fn is_optimize_due(state: &Map<String, Value>, key: &str, today: &str) -> bool {
    state.get(key).and_then(Value::as_str) != Some(today)
}

/// One optimize step's outcome (v4 `OptimizeStepResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub name: &'static str,
    pub ok: bool,
    pub ms: u64,
    pub error: Option<String>,
}

/// v4 `optimizeDatabase`'s return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizeOutcome {
    pub ok: bool,
    pub steps: Vec<StepResult>,
}

impl OptimizeOutcome {
    /// The steps as v4's array (`{name, ok, ms, error?}`) — the `steps` field
    /// of `Database optimize finished`.
    pub fn steps_json(&self) -> Value {
        Value::Array(
            self.steps
                .iter()
                .map(|s| {
                    let mut o = Map::new();
                    o.insert("name".into(), Value::from(s.name));
                    o.insert("ok".into(), Value::from(s.ok));
                    o.insert("ms".into(), Value::from(s.ms));
                    if let Some(e) = &s.error {
                        o.insert("error".into(), Value::from(e.as_str()));
                    }
                    Value::Object(o)
                })
                .collect(),
        )
    }
}

/// The optimize steps, in order: `VACUUM`, `ANALYZE`, `PRAGMA optimize`.
pub const OPTIMIZE_STEPS: [&str; 3] = ["VACUUM", "ANALYZE", "PRAGMA optimize"];

/// Run the CLI's optimize steps against an open connection, stopping at the
/// first failing step, WITHOUT logging (so a writer-thread caller can log on
/// its own thread — see the module doc). Each step in autocommit.
pub fn run_optimize_steps(conn: &Connection) -> OptimizeOutcome {
    let mut steps = Vec::new();
    for name in OPTIMIZE_STEPS {
        let t0 = Instant::now();
        let result = conn.execute_batch(name);
        let ms = t0.elapsed().as_millis() as u64;
        match result {
            Ok(()) => steps.push(StepResult {
                name,
                ok: true,
                ms,
                error: None,
            }),
            Err(e) => {
                steps.push(StepResult {
                    name,
                    ok: false,
                    ms,
                    error: Some(e.to_string()),
                });
                return OptimizeOutcome { ok: false, steps };
            }
        }
    }
    OptimizeOutcome { ok: true, steps }
}

/// v4 `optimizeDatabase`'s per-step lines, for an outcome already run.
pub fn log_optimize_steps(label: &str, outcome: &OptimizeOutcome) {
    for step in &outcome.steps {
        match &step.error {
            None => tracing::debug!(
                target: "quilltap::startup",
                module = DAILY_DB_OPTIMIZE_MODULE,
                database = label,
                step = step.name,
                ms = step.ms,
                "Optimize step complete"
            ),
            Some(error) => tracing::error!(
                target: "quilltap::startup",
                module = DAILY_DB_OPTIMIZE_MODULE,
                database = label,
                step = step.name,
                ms = step.ms,
                error = error.as_str(),
                "Optimize step failed"
            ),
        }
    }
}

/// v4 `optimizeDatabase` — run the steps and log them, on the caller's thread.
pub fn optimize_database(conn: &Connection, label: &str) -> OptimizeOutcome {
    let outcome = run_optimize_steps(conn);
    log_optimize_steps(label, &outcome);
    outcome
}

/// What one pass did (for the host's tests and the boot record).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DailyOptimizeReport {
    /// Nothing was due (the skip DEBUG).
    pub skipped: bool,
    /// The keys stamped today by this pass.
    pub stamped: Vec<String>,
    pub optimized: u64,
    pub attempted: u64,
    pub reclaimed_bytes: u64,
}

/// One target of the pass.
struct Target {
    key: &'static str,
    partition: Partition,
    kind: BackupKind,
    file: &'static str,
}

const TARGETS: [Target; 3] = [
    Target {
        key: "main",
        partition: Partition::Main,
        kind: BackupKind::Main,
        file: "quilltap.db",
    },
    Target {
        key: "llm-logs",
        partition: Partition::LlmLogs,
        kind: BackupKind::LlmLogs,
        file: "quilltap-llm-logs.db",
    },
    Target {
        key: "mount-points",
        partition: Partition::MountIndex,
        kind: BackupKind::MountIndex,
        file: "quilltap-mount-index.db",
    },
];

/// `fs.statSync(p).size`, 0 on error.
fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// The target's pre-optimize backup over its read pool connection.
fn pre_optimize_backup(
    db: &Db,
    target: &Target,
    data_dir: &Path,
    now_ms: i64,
    zone: &TimeZone,
) -> Result<Result<Option<PathBuf>, String>, DbError> {
    let run = |c: &Connection| {
        Ok(physical_backup::create_physical_backup(
            c,
            data_dir,
            target.kind,
            now_ms,
            zone,
        ))
    };
    match target.partition {
        Partition::Main => db.read_main(run),
        Partition::LlmLogs => db.read_llm_logs(run),
        Partition::MountIndex => db.read_mount_index(run),
    }
}

/// The three steps on the target's WRITER, in one closure.
fn optimize_on_writer(db: &Db, partition: Partition) -> Result<OptimizeOutcome, DbError> {
    db.write_blocking(move |ws: &mut WriterSet| {
        let writer = match partition {
            Partition::Main => Some(ws.main()),
            Partition::LlmLogs => ws.llm_logs(),
            Partition::MountIndex => ws.mount_index(),
        };
        let writer = writer.ok_or(DbError::PartitionUnavailable(match partition {
            Partition::Main => crate::write_partition::WriteDbTarget::Main,
            Partition::LlmLogs => crate::write_partition::WriteDbTarget::LlmLogs,
            Partition::MountIndex => crate::write_partition::WriteDbTarget::MountIndex,
        }))?;
        Ok(run_optimize_steps(writer.connection()))
    })
}

/// The per-target ERROR arm (v4's outer catch) — NOT stamped.
fn log_target_failed(label: &str, error: &str) {
    tracing::error!(
        target: "quilltap::startup",
        module = DAILY_DB_OPTIMIZE_MODULE,
        database = label,
        error,
        "Database optimize failed; will retry on next launch"
    );
}

/// v4 `runDailyDbOptimize` — the daily backup + optimize pass. `now_ms` and
/// `zone` decide "today" (and the backups' names); `data_dir` holds the state
/// file, the databases and `backups/`. Blocks (it calls `write_blocking`):
/// never call it from a tokio worker. Never fails.
pub fn run_daily_db_optimize(
    db: &Db,
    data_dir: &Path,
    zone: &TimeZone,
    now_ms: i64,
) -> DailyOptimizeReport {
    let today = local_date_stamp(now_ms, zone);
    let state_path = optimize_state_path(data_dir);
    let mut state = read_optimize_state(&state_path);
    let mut report = DailyOptimizeReport::default();

    let work: Vec<&Target> = TARGETS
        .iter()
        .filter(|t| is_optimize_due(&state, t.key, &today))
        .collect();
    if work.is_empty() {
        let state_json = Value::Object(state).to_string();
        tracing::debug!(
            target: "quilltap::startup",
            module = DAILY_DB_OPTIMIZE_MODULE,
            today = today.as_str(),
            stateJson = state_json.as_str(),
            "Databases already optimized today; skipping"
        );
        report.skipped = true;
        return report;
    }

    let databases = Value::Array(work.iter().map(|t| Value::from(t.key)).collect());
    tracing::info!(
        target: "quilltap::startup",
        module = DAILY_DB_OPTIMIZE_MODULE,
        today = today.as_str(),
        databasesJson = databases.to_string().as_str(),
        "Daily database optimize starting"
    );

    let t0 = Instant::now();
    for target in &work {
        let label = target.key;
        match db.partition_state(target.partition) {
            PartitionState::Absent => {
                tracing::debug!(
                    target: "quilltap::startup",
                    module = DAILY_DB_OPTIMIZE_MODULE,
                    database = label,
                    "Database file not present; nothing to optimize"
                );
                state.insert(label.to_string(), Value::String(today.clone()));
                report.stamped.push(label.to_string());
                continue;
            }
            PartitionState::Degraded => {
                // DEGRADED_OPTIMIZE_ERROR_TEXT (module doc).
                let unavailable = DbError::PartitionUnavailable(match target.partition {
                    Partition::Main => crate::write_partition::WriteDbTarget::Main,
                    Partition::LlmLogs => crate::write_partition::WriteDbTarget::LlmLogs,
                    Partition::MountIndex => crate::write_partition::WriteDbTarget::MountIndex,
                });
                log_target_failed(label, &crate::db::fallback::error_text(&unavailable));
                continue;
            }
            PartitionState::Open => {}
        }

        match pre_optimize_backup(db, target, data_dir, now_ms, zone) {
            Ok(Ok(backup_path)) => {
                let shown = backup_path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| BACKUP_SKIPPED_TEXT.to_string());
                tracing::debug!(
                    target: "quilltap::startup",
                    module = DAILY_DB_OPTIMIZE_MODULE,
                    database = label,
                    backupPath = shown.as_str(),
                    "Pre-optimize backup step done"
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(
                    target: "quilltap::startup",
                    module = DAILY_DB_OPTIMIZE_MODULE,
                    database = label,
                    error = error.as_str(),
                    "Pre-optimize backup threw; optimizing anyway (VACUUM is transactional)"
                );
            }
            Err(e) => {
                log_target_failed(label, &crate::db::fallback::error_text(&e));
                continue;
            }
        }

        let db_path = data_dir.join(target.file);
        let size_before = file_size(&db_path);
        let outcome = match optimize_on_writer(db, target.partition) {
            Ok(o) => o,
            Err(e) => {
                log_target_failed(label, &crate::db::fallback::error_text(&e));
                continue;
            }
        };
        log_optimize_steps(label, &outcome);
        // NO `wal_checkpoint(TRUNCATE)` — WAL_CHECKPOINT_UNDER_TRUNCATE (module doc).
        let size_after = file_size(&db_path);

        if outcome.ok {
            state.insert(label.to_string(), Value::String(today.clone()));
            report.stamped.push(label.to_string());
            report.optimized += 1;
            report.reclaimed_bytes += size_before.saturating_sub(size_after);
        }
        tracing::info!(
            target: "quilltap::startup",
            module = DAILY_DB_OPTIMIZE_MODULE,
            database = label,
            ok = outcome.ok,
            sizeBefore = size_before,
            sizeAfter = size_after,
            stepsJson = outcome.steps_json().to_string().as_str(),
            "Database optimize finished"
        );
    }

    write_optimize_state(&state, &state_path);

    report.attempted = work.len() as u64;
    tracing::info!(
        target: "quilltap::startup",
        module = DAILY_DB_OPTIMIZE_MODULE,
        optimized = report.optimized,
        attempted = report.attempted,
        reclaimedBytes = report.reclaimed_bytes,
        elapsedMs = t0.elapsed().as_millis() as u64,
        "Daily database optimize complete"
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v4's own test rows (`daily-db-optimize.test.ts:36-39`), in v4's local
    /// zone pinned to UTC; the zone is an argument.
    #[test]
    fn stamps_the_local_calendar_date() {
        let utc = TimeZone::UTC;
        // new Date(2026, 0, 5, 23, 59) / new Date(2026, 11, 31, 0, 1) under UTC
        assert_eq!(local_date_stamp(1_767_657_540_000, &utc), "2026-01-05");
        assert_eq!(local_date_stamp(1_798_675_260_000, &utc), "2026-12-31");
        let chicago = TimeZone::get("America/Chicago").unwrap();
        // 2026-10-09T03:30Z is still the 8th in Chicago.
        assert_eq!(local_date_stamp(1_791_516_600_000, &chicago), "2026-10-08");
    }

    #[test]
    fn due_only_when_not_stamped_today() {
        let mut state = Map::new();
        state.insert("main".into(), "2026-10-07".into());
        state.insert("llm-logs".into(), "2026-10-06".into());
        assert!(!is_optimize_due(&state, "main", "2026-10-07"));
        assert!(is_optimize_due(&state, "llm-logs", "2026-10-07"));
        assert!(is_optimize_due(&state, "mount-points", "2026-10-07"));
    }

    /// v4's round-trip test (`:49-55`): unknown keys dropped on read; the
    /// written bytes are `JSON.stringify(state, null, 2) + '\n'`.
    #[test]
    fn round_trips_state_and_drops_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(OPTIMIZE_STATE_FILENAME);
        let mut state = Map::new();
        state.insert("main".into(), "2026-10-07".into());
        state.insert("llm-logs".into(), "2026-10-06".into());
        write_optimize_state(&state, &path);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\n  \"main\": \"2026-10-07\",\n  \"llm-logs\": \"2026-10-06\"\n}\n"
        );
        std::fs::write(
            &path,
            r#"{"main":"2026-10-07","llm-logs":"2026-10-06","bogus":"x"}"#,
        )
        .unwrap();
        assert_eq!(read_optimize_state(&path), state);
        write_optimize_state(&Map::new(), &path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\n");
    }

    /// v4's step-order and stop-at-first-failure tests (`:66-87`) over a real
    /// connection: a read-only handle refuses the VACUUM, and nothing after it
    /// runs.
    #[test]
    fn steps_run_in_order_and_stop_at_the_first_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE t (x); CREATE INDEX t_x ON t (x); INSERT INTO t VALUES (1);",
            )
            .unwrap();
            let o = run_optimize_steps(&c);
            assert!(o.ok);
            assert_eq!(
                o.steps.iter().map(|s| s.name).collect::<Vec<_>>(),
                OPTIMIZE_STEPS.to_vec()
            );
        }
        let ro =
            Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let o = run_optimize_steps(&ro);
        assert!(!o.ok);
        assert_eq!(o.steps.len(), 1);
        assert_eq!(o.steps[0].name, "VACUUM");
        assert_eq!(
            o.steps[0].error.as_deref(),
            Some("attempt to write a readonly database")
        );
    }
}
