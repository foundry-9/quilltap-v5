//! P4.D259 — v4 `f5e953a3f`'s PHASE 0.75 daily backup + optimize at the head
//! of the host's `assemble`, and v4's PHASE-2 startup backup + retention as a
//! fire-and-forget after the pumps, through the REAL `Host::start`.
//!
//! The pass's bytes and lines are proven against v4 by the tier-2 family
//! (`crates/quilltap-harness/tests/daily_db_optimize_equivalence.rs`); these
//! arms prove the WIRING: that a boot runs the pass on the writer connections
//! before anything else, stamps `data/db-optimize-state.json` in v4's bytes,
//! leaves the trio in `data/backups/`, never fails the boot, and that the
//! startup trio runs after it (finding the PHASE-0.75 files and skipping).
//!
//! Seven arms (the order's item 8): (a) a first boot; (b) a second boot the
//! same day; (c) an unwritable state file; (d) a corrupt state file; (e) a
//! garbage LLM-logs sibling; (f) a main-only instance; (g) a 25-hour-old
//! planted backup. Arm (c) plants a DIRECTORY at the state path rather than
//! making `data/` read-only as the order sketched: a read-only `data/` fails
//! the instance lock's own write before the pass can run (`pre_open` writes
//! `data/quilltap.lock`), so it would test the lock, not the WARN.
//!
//! Lines are read through a process-global capture (the pass runs on a fresh
//! OS thread inside `assemble`, the startup trio on a blocking task), so the
//! boots are serialized.
//!
//! Red-first (the lane record): on `main` before the wiring every arm failed —
//! no state file was ever written and no `data/backups/` directory existed.
//!
//! Run:
//!   cargo test -p quilltap-host --test host_boot_daily_optimize

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use quilltap_core::api::{QuilltapCore as _, Request, Response};
use quilltap_core::db::Writer;
use quilltap_core::host_zone::TimeZone;
use quilltap_core::services::daily_db_optimize::local_date_stamp;
use quilltap_core::services::physical_backup::{generate_backup_filename, BackupKind};
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const MOUNT: &str = "quilltap-mount-index.db";
const LLM: &str = "quilltap-llm-logs.db";
const STATE: &str = "db-optimize-state.json";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn capture() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn lines() -> Vec<String> {
    capture().lock().unwrap().clone()
}

fn count(lines: &[String], needle: &str) -> usize {
    lines.iter().filter(|l| l.contains(needle)).count()
}

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config.set_display_zone(TimeZone::UTC);
    config
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn today() -> String {
    local_date_stamp(now_ms(), &TimeZone::UTC)
}

/// v4's bytes for a state stamped `keys` today.
fn stamped(keys: &[&str]) -> String {
    let today = today();
    let body: Vec<String> = keys
        .iter()
        .map(|k| format!("  \"{k}\": \"{today}\""))
        .collect();
    format!("{{\n{}\n}}\n", body.join(",\n"))
}

fn fresh() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    (dir, data)
}

/// Boot the real host (capture cleared first), check it answers `Health`, and
/// wait for the startup trio's retention pass to log `sets` lines — the
/// fire-and-forget must not outlive the test's temp dir.
async fn boot(base: &Path, sets: usize) -> Host {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot never fails on the daily pass");
    match host.core().dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready, "the boot finished but is not ready"),
        other => panic!("unexpected: {other:?}"),
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while count(&lines(), "Retention policy applied") < sets {
        assert!(
            Instant::now() < deadline,
            "the startup backup's retention never logged {sets} set(s):\n{:#?}",
            lines()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    host
}

fn backups(data: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(data.join("backups"))
        .map(|rd| {
            rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn opens_keyed(path: &Path) -> bool {
    let w = Writer::open_writable(path, PEPPER).unwrap();
    w.connection()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false)
}

fn assert_never_logged(lines: &[String]) {
    for line in [
        "Post-optimize WAL checkpoint failed",
        "Daily database optimize failed — continuing startup",
        "Error closing database after optimize",
        "Not a SQLite backend",
        "Startup physical backup or retention policy failed",
        "LLM logs startup physical backup failed",
        "Mount index startup physical backup failed",
    ] {
        assert_eq!(count(lines, line), 0, "{line} logged:\n{lines:#?}");
    }
}

#[tokio::test]
async fn a_first_boot_backs_up_optimizes_and_stamps_all_three() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    let _host = boot(dir.path(), 3).await;
    let l = lines();

    assert_eq!(
        std::fs::read_to_string(data.join(STATE)).unwrap(),
        stamped(&["main", "llm-logs", "mount-points"])
    );
    let names = backups(&data);
    assert_eq!(names.len(), 3, "{names:?}");
    for (kind, name) in [
        // bytewise: `quilltap-2…` < `quilltap-llm-logs-…` < `quilltap-mount-index-…`
        (BackupKind::Main, &names[0]),
        (BackupKind::LlmLogs, &names[1]),
        (BackupKind::MountIndex, &names[2]),
    ] {
        assert!(name.starts_with(kind.prefix()), "{name}");
        assert!(
            opens_keyed(&data.join("backups").join(name)),
            "{name} keyed"
        );
    }
    // The pass, in order, before the seeds; then the startup trio skipping.
    let starting = l
        .iter()
        .position(|x| x.contains("Daily database optimize starting"))
        .unwrap();
    let complete = l
        .iter()
        .position(|x| x.contains("Daily database optimize complete"))
        .unwrap();
    assert!(starting < complete);
    assert_eq!(count(&l, "Database optimize finished"), 3);
    assert_eq!(
        l.iter()
            .filter(|x| x.contains("Database optimize finished") && x.contains(" ok=true "))
            .count(),
        3,
        "{l:#?}"
    );
    for label in ["main database", "LLM logs", "mount index"] {
        assert_eq!(
            count(&l, &format!("Recent {label} backup exists, skipping")),
            1
        );
    }
    assert_never_logged(&l);
    // ANALYZE ran on the writer: main now carries statistics.
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let stat1: i64 = w
        .connection()
        .query_row("SELECT count(*) FROM sqlite_stat1", [], |r| r.get(0))
        .unwrap();
    assert!(stat1 > 0);
}

#[tokio::test]
async fn a_second_boot_the_same_day_writes_nothing() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    let first = boot(dir.path(), 3).await;
    drop(first);
    let state_path = data.join(STATE);
    let bytes = std::fs::read(&state_path).unwrap();
    let mtime = std::fs::metadata(&state_path).unwrap().modified().unwrap();
    let names = backups(&data);

    let _second = boot(dir.path(), 3).await;
    let l = lines();
    assert_eq!(std::fs::read(&state_path).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(&state_path).unwrap().modified().unwrap(),
        mtime
    );
    assert_eq!(backups(&data), names, "no new backup");
    assert_eq!(count(&l, "Databases already optimized today; skipping"), 1);
    assert_eq!(count(&l, "Daily database optimize starting"), 0);
    assert_never_logged(&l);
}

#[tokio::test]
async fn an_unwritable_state_file_warns_and_the_boot_answers() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    std::fs::create_dir(data.join(STATE)).unwrap();
    let _host = boot(dir.path(), 3).await;
    let l = lines();
    assert_eq!(
        count(
            &l,
            "Could not read database optimize state; treating every database as due"
        ),
        1
    );
    assert_eq!(
        count(
            &l,
            "Could not write database optimize state; optimize will repeat on next launch"
        ),
        1
    );
    assert_eq!(
        count(&l, "EISDIR: illegal operation on a directory"),
        2,
        "{l:#?}"
    );
    assert_eq!(count(&l, "Database optimize finished"), 3);
    assert!(data.join(STATE).is_dir());
    assert_eq!(backups(&data).len(), 3);
    assert_never_logged(&l);
}

#[tokio::test]
async fn a_corrupt_state_file_warns_and_runs_the_full_pass() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    std::fs::write(data.join(STATE), "{not json").unwrap();
    let _host = boot(dir.path(), 3).await;
    let l = lines();
    assert_eq!(
        count(
            &l,
            "error=Expected property name or '}' in JSON at position 1 (line 1 column 2)"
        ),
        1,
        "{l:#?}"
    );
    assert_eq!(count(&l, "Database optimize finished"), 3);
    assert_eq!(
        std::fs::read_to_string(data.join(STATE)).unwrap(),
        stamped(&["main", "llm-logs", "mount-points"])
    );
    assert_never_logged(&l);
}

#[tokio::test]
async fn a_garbage_llm_logs_sibling_is_not_stamped_and_the_boot_degrades() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    let garbage: Vec<u8> = (0..4608usize)
        .map(|i| ((i * 131 + 17) & 255) as u8)
        .collect();
    std::fs::write(data.join(LLM), garbage).unwrap();
    // The LLM-logs partition is degraded: its backup is skipped, so retention
    // logs two sets.
    let _host = boot(dir.path(), 2).await;
    let l = lines();
    assert_eq!(
        count(
            &l,
            "Database optimize failed; will retry on next launch module=startup:daily-db-optimize database=llm-logs error=LLM logs database is in degraded mode"
        ),
        1,
        "{l:#?}"
    );
    assert_eq!(
        std::fs::read_to_string(data.join(STATE)).unwrap(),
        stamped(&["main", "mount-points"])
    );
    let names = backups(&data);
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names
        .iter()
        .all(|n| !n.starts_with(BackupKind::LlmLogs.prefix())));
    assert_never_logged(&l);
}

#[tokio::test]
async fn a_main_only_instance_stamps_both_siblings_as_absent() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    std::fs::remove_file(data.join(LLM)).unwrap();
    std::fs::remove_file(data.join(MOUNT)).unwrap();
    let _host = boot(dir.path(), 1).await;
    let l = lines();
    assert_eq!(
        count(&l, "Database file not present; nothing to optimize"),
        2
    );
    assert_eq!(
        std::fs::read_to_string(data.join(STATE)).unwrap(),
        stamped(&["main", "llm-logs", "mount-points"])
    );
    let names = backups(&data);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].starts_with(BackupKind::Main.prefix()));
    assert_never_logged(&l);
}

#[tokio::test]
async fn a_day_old_backup_is_replaced_and_kept() {
    let _serial = SERIAL.lock().await;
    let (dir, data) = fresh();
    let old = generate_backup_filename(BackupKind::Main, now_ms() - 25 * 3_600_000, &TimeZone::UTC);
    std::fs::create_dir(data.join("backups")).unwrap();
    std::fs::write(data.join("backups").join(&old), "an old backup").unwrap();
    let _host = boot(dir.path(), 3).await;
    let l = lines();
    assert_eq!(
        count(&l, "Last main database backup is old enough, backup needed"),
        1
    );
    assert_eq!(count(&l, "ageHours=25"), 1, "{l:#?}");
    let names = backups(&data);
    let mains: Vec<_> = names
        .iter()
        .filter(|n| BackupKind::Main.prefix().len() + 17 + 3 == n.len())
        .collect();
    assert_eq!(mains.len(), 2, "{names:?}");
    assert!(names.contains(&old), "the < 7 d backup is kept");
    assert_never_logged(&l);
}
