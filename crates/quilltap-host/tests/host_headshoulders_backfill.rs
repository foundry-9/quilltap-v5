//! P4.82 integration test: the `CHARACTER_HEADSHOULDERS_BACKFILL` handler is
//! REGISTERED in the production spine, and the one-time backfill SCAN actually
//! runs on the host's boot-repair thread.
//!
//! Both differentials (`headshoulders_backfill_tier3_equivalence` and
//! `headshoulders_backfill_enqueue_equivalence`) prove the ported code against
//! v4's real code by CALLING it directly, so neither can see whether the host
//! ever does. That gap is exactly dogfood finding #40's shape: `LLM_LOG_CLEANUP`
//! sat in `KNOWN_JOB_TYPES` with a live enqueue path and no registration for
//! rounds, burning three attempts per boot against the "recognized but not yet
//! available" arm. A registration is one line, and one line is what nobody
//! notices going missing.
//!
//! Both tests run over a COPY of the committed `headshoulders-{main,mount}.db`
//! pair — the same substrate the two differentials use, so a character here is
//! a character there.
//!
//! **The registration test deliberately picks the arm that touches no
//! provider.** `Profileless Perpetua`'s user owns no connection profile, so the
//! handler resolves the character through the real vault overlay, reads the
//! profiles, finds none, warns and RETURNS — a job that COMPLETES with zero
//! outbound work. The scan is disarmed for that test (its flag is pre-set), so
//! the only job in flight is the seeded one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_host::spine::ProductionSpineFactory;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
/// `Profileless Perpetua` — her user owns no connection profile, so the handler
/// reaches the "No connection profile configured, skipping" arm and completes
/// without a single outbound call.
const PROFILELESS_CHARACTER: &str = "b3000082-0000-4000-8000-00000000000b";
const PROFILELESS_USER: &str = "e0000082-0000-4000-8000-000000000004";
const JOB: &str = "a0000082-0000-4000-8000-0000000000c1";
const FLAG_KEY: &str = "headshoulders_backfill_enqueued_v1";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// Copy the committed pair into a host-shaped instance directory.
fn make_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("headshoulders-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("headshoulders-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
}

fn write_flag(base: &Path, value: &str) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    w.connection()
        .execute(
            "INSERT INTO instance_settings (\"key\", \"value\") VALUES (?1, ?2) \
             ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
            rusqlite::params![FLAG_KEY, value],
        )
        .unwrap();
}

fn seed_job(base: &Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    w.connection()
        .execute(
            "INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, \
              maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
             VALUES (?1, ?2, 'CHARACTER_HEADSHOULDERS_BACKFILL', 'PENDING', ?3, -1, 0, 3, NULL, \
                     '2020-01-01T00:00:00.000Z', NULL, NULL, \
                     '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            rusqlite::params![
                JOB,
                PROFILELESS_USER,
                format!(r#"{{"characterId":"{PROFILELESS_CHARACTER}"}}"#)
            ],
        )
        .unwrap();
}

fn quiet_config(base: &Path, with_spine: bool) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    let hour = 3_600_000;
    config.autonomous_tick_ms = hour;
    config.stuck_check_ms = hour;
    config.cleanup_interval_ms = hour;
    config.housekeeping_interval_ms = hour;
    config.maintenance_interval_ms = hour;
    config.danger_scan_interval_ms = hour;
    config.startup_grace_ms = hour;
    config.heartbeat_ms = hour;
    config.terminal = false;
    config.seed_sample_content = false;
    if with_spine {
        config.spine = Some(std::sync::Arc::new(ProductionSpineFactory::new(
            base.to_path_buf(),
            "0.0.0-test".to_string(),
            "UTC".to_string(),
        )));
    }
    config
}

async fn wait_until(mut probe: impl FnMut() -> bool, what: &str) {
    for _ in 0..400 {
        if probe() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn job_status(db: &Db) -> (String, Option<String>) {
    db.read_main(|c| {
        Ok(c.query_row(
            "SELECT status, lastError FROM background_jobs WHERE id = ?1",
            rusqlite::params![JOB],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )?)
    })
    .unwrap()
}

fn backfill_job_count(db: &Db) -> i64 {
    db.read_main(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM background_jobs WHERE type = 'CHARACTER_HEADSHOULDERS_BACKFILL'",
            [],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .unwrap()
}

fn flag(db: &Db) -> Option<String> {
    db.read_main(|c| Ok(quilltap_core::services::headshoulders_backfill_enqueue::flag_value(c)))
        .unwrap()
}

/// The registration is live: a PENDING `CHARACTER_HEADSHOULDERS_BACKFILL` job
/// runs through the production spine to COMPLETED.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn seeded_backfill_job_runs_through_the_production_spine() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    // Disarm the boot scan so the seeded job is the only one in flight.
    write_flag(dir.path(), "true");
    seed_job(dir.path());

    let host = Host::start(quiet_config(dir.path(), true)).unwrap();
    let db = host.core().db().unwrap();

    wait_until(
        || {
            let s = job_status(&db).0;
            s != "PENDING" && s != "PROCESSING"
        },
        "the backfill job to leave the queue",
    )
    .await;

    let (status, last_error) = job_status(&db);
    assert_eq!(
        (status.as_str(), last_error.as_deref()),
        ("COMPLETED", None),
        "the job did not complete — if lastError mentions \"not yet available\", \
         the ProductionSpineFactory registration was dropped"
    );
}

/// The boot-repair thread runs the one-time scan: a fresh instance gains a job
/// per eligible character AND the flag; a second boot adds nothing; and an
/// instance whose flag v4 already wrote is left completely alone.
///
/// No spine is wired, so the enqueued jobs meet the runner's loud fallback and
/// fail harmlessly without touching a provider — this test is about the ROWS,
/// which are never deleted, not about their status.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_runs_the_one_time_backfill_scan_exactly_once() {
    // The committed pair carries eleven characters, seven of them eligible,
    // plus one already-PENDING backfill job whose character the dedupe skips.
    const SEEDED: i64 = 1;
    const ELIGIBLE: i64 = 7;

    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());

    // First boot: the scan runs.
    {
        let host = Host::start(quiet_config(dir.path(), false)).unwrap();
        let db = host.core().db().unwrap();
        wait_until(|| flag(&db).is_some(), "the backfill flag to be written").await;
        assert_eq!(
            backfill_job_count(&db),
            SEEDED + ELIGIBLE,
            "the boot scan should have enqueued one job per eligible character — \
             if this is {SEEDED}, the seed_built_ins call was dropped"
        );
        assert_eq!(flag(&db).as_deref(), Some("true"));
        drop(host);
    }

    // Second boot on the same instance: the flag gates the scan.
    {
        let host = Host::start(quiet_config(dir.path(), false)).unwrap();
        let db = host.core().db().unwrap();
        wait_until(|| flag(&db).is_some(), "the instance to come up").await;
        assert_eq!(
            backfill_job_count(&db),
            SEEDED + ELIGIBLE,
            "a second boot must enqueue nothing"
        );
        drop(host);
    }

    // A pristine copy whose flag v4 already wrote — the cross-app shape. v4 has
    // set this row on every instance it has booted since the feature shipped,
    // so a v5 boot there must scan nothing and write nothing.
    {
        let dir2 = tempfile::tempdir().unwrap();
        make_instance(dir2.path());
        write_flag(dir2.path(), "true");
        let host = Host::start(quiet_config(dir2.path(), false)).unwrap();
        let db = host.core().db().unwrap();
        wait_until(|| flag(&db).is_some(), "the instance to come up").await;
        assert_eq!(
            backfill_job_count(&db),
            SEEDED,
            "a v4-written flag must leave the instance untouched"
        );
        drop(host);
    }
}
