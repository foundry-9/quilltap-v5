//! P4.24 integration test: `LLM_LOG_CLEANUP` is REGISTERED in the production
//! spine and a boot-minted job actually completes.
//!
//! The differential (`llm_log_cleanup_equivalence`) proves the handler against
//! v4's real code; it drives the handler directly and so cannot see whether the
//! host ever calls it. That gap is the whole of dogfood finding #40: the enqueue
//! path had been live on the daily cadence AND at boot since P4.1d while
//! `LLM_LOG_CLEANUP` was the last type in `KNOWN_JOB_TYPES` with no handler, so
//! every start-up minted a job that burned three attempts against the
//! "recognized but not yet available" arm and died. A registration is one line,
//! and one line is exactly what nobody notices going missing — hence this test.
//!
//! It boots a real [`Host`] over a real encrypted two-partition instance with
//! the real [`ProductionSpineFactory`], seeds one PENDING job, and asserts the
//! job COMPLETES **and** the over-age log row is gone. Nothing here touches a
//! provider: this handler's only outbound work is a DELETE.
//!
//! The instance is hand-rolled with loose-typed tables, like `host_cadence.rs` —
//! the tier-2 differential owns real-schema fidelity; this owns the wiring.

use std::path::Path;
use std::time::Duration;

use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_host::spine::ProductionSpineFactory;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
const JOB: &str = "a0000000-0000-4000-8000-0000000000c1";

const BACKGROUND_JOBS_DDL: &str = "CREATE TABLE background_jobs (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
    payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
    maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
    startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

const INSTANCE_SETTINGS_DDL: &str = "CREATE TABLE instance_settings (\
    \"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);";

const CHAT_SETTINGS_DDL: &str = "CREATE TABLE chat_settings (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, autoHousekeepingSettings TEXT, \
    llmLoggingSettings TEXT, dangerousContentSettings TEXT, \
    createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT NOT NULL, \
    modelName TEXT NOT NULL, connectionProfileId TEXT, imageProfileId TEXT, \
    request TEXT NOT NULL, response TEXT NOT NULL, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, durationMs REAL, \
    createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

/// A two-partition instance: one user with a 7-day retention window, one PENDING
/// cleanup job, and two log rows — one from 2020 (far past any cutoff) and one
/// stamped "now-ish" by the far-future `createdAt` below, which no plausible
/// wall clock can age out.
fn make_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();

    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    let c = w.connection();
    c.execute_batch(BACKGROUND_JOBS_DDL).unwrap();
    c.execute_batch(INSTANCE_SETTINGS_DDL).unwrap();
    c.execute_batch(CHAT_SETTINGS_DDL).unwrap();
    c.execute(
        "INSERT INTO chat_settings (id, userId, llmLoggingSettings, createdAt, updatedAt) \
         VALUES ('a0000000-0000-4000-8000-00000000000a', ?1, \
                 '{\"enabled\":true,\"verboseMode\":false,\"retentionDays\":7}', \
                 '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        rusqlite::params![USER],
    )
    .unwrap();
    c.execute(
        "INSERT INTO background_jobs (id, userId, type, status, payload, priority, attempts, \
          maxAttempts, lastError, scheduledAt, startedAt, completedAt, createdAt, updatedAt) \
         VALUES (?1, ?2, 'LLM_LOG_CLEANUP', 'PENDING', ?3, 0, 0, 3, NULL, \
                 '2020-01-01T00:00:00.000Z', NULL, NULL, \
                 '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
        rusqlite::params![JOB, USER, format!(r#"{{"userId":"{USER}"}}"#)],
    )
    .unwrap();
    drop(w);

    let l = Writer::open_writable(&data.join("quilltap-llm-logs.db"), PEPPER).unwrap();
    let lc = l.connection();
    lc.execute_batch(LLM_LOGS_DDL).unwrap();
    for (id, created) in [
        (
            "b0000000-0000-4000-8000-0000000000d1",
            "2020-01-01T00:00:00.000Z",
        ),
        (
            "b0000000-0000-4000-8000-0000000000d2",
            "2999-01-01T00:00:00.000Z",
        ),
    ] {
        lc.execute(
            "INSERT INTO llm_logs (id, userId, type, provider, modelName, request, response, \
              createdAt, updatedAt) \
             VALUES (?1, ?2, 'CHAT_MESSAGE', 'OPENAI_COMPATIBLE', 'mock', '{}', '{}', ?3, ?3)",
            rusqlite::params![id, USER, created],
        )
        .unwrap();
    }
}

fn quiet_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    let hour = 3_600_000;
    // Every cadence pushed out of the way — including the cleanup sweep, whose
    // boot tick would otherwise enqueue a SECOND job and muddy the assertion.
    config.autonomous_tick_ms = hour;
    config.stuck_check_ms = hour;
    config.cleanup_interval_ms = hour;
    config.housekeeping_interval_ms = hour;
    config.maintenance_interval_ms = hour;
    config.danger_scan_interval_ms = hour;
    config.startup_grace_ms = hour;
    config.heartbeat_ms = hour;
    config.spine = Some(std::sync::Arc::new(ProductionSpineFactory::new(
        base.to_path_buf(),
        "0.0.0-test".to_string(),
        "UTC".to_string(),
    )));
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

fn surviving_log_ids(db: &Db) -> Vec<String> {
    db.read_llm_logs(|c| {
        let mut stmt = c.prepare("SELECT id FROM llm_logs ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    })
    .unwrap()
}

/// The registration is live: a PENDING `LLM_LOG_CLEANUP` job runs to COMPLETED
/// and takes the over-age log row with it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_minted_cleanup_job_completes_and_prunes() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();

    wait_until(
        || job_status(&db).0 != "PENDING" && job_status(&db).0 != "PROCESSING",
        "the cleanup job to leave the queue",
    )
    .await;

    let (status, last_error) = job_status(&db);
    assert_eq!(
        (status.as_str(), last_error.as_deref()),
        ("COMPLETED", None),
        "the job did not complete — if lastError mentions \"not yet available\", \
         the ProductionSpineFactory registration was dropped"
    );
    assert_eq!(
        surviving_log_ids(&db),
        ["b0000000-0000-4000-8000-0000000000d2"],
        "the over-age log row should have been pruned and the fresh one kept"
    );
}
