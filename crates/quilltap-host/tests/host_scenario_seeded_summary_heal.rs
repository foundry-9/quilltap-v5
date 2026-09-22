//! P4.D208 integration test: bug 158's clear pass actually RUNS on the host's
//! boot-repair thread.
//!
//! The pass's own unit tests prove the predicate and the shared SQL by calling
//! them directly, so they cannot see whether the host ever does — the P4.D175
//! gap, and the class the §3 reviews keep catching (a wiring is one call, and
//! one call is what nobody notices going missing). It matters here because the
//! heal is the fix's retrospective half: while the scenario sits in
//! `contextSummary`, `find_recent_summarized_by_character` keeps handing the
//! greeting a stage direction to open from, for every chat already on disk.
//!
//! Runs over a COPY of the committed `chat-scenario-{main,mount}.db` pair with
//! four chats PLANTED — the pair carries no seeded row naturally, and a test
//! that planted nothing would pass against a deleted call.

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MIGRATION_ID: &str = "clear-scenario-seeded-chat-summaries-v1";

const SCENARIO: &str = "# Scenario: Amy's Pool\n\nAmy is in her pool.";

/// Seeded: the two columns hold the same bytes. Must be cleared.
const SEEDED_A: &str = "d0000208-0000-4000-8000-000000000001";
/// Seeded too, with different bytes — so `cleared` is 2, not 1, and the
/// singular/plural arm of v4's message is the one that does NOT fire.
const SEEDED_B: &str = "d0000208-0000-4000-8000-000000000002";
/// A real summary that QUOTES the scenario. Must survive.
const REAL_SUMMARY: &str = "d0000208-0000-4000-8000-000000000003";
/// A whitespace DIFFERENCE, not a match. Must survive — the predicate is byte
/// equality, and a `TRIM()` in the SQL would take this row.
const WHITESPACE_DIFFERS: &str = "d0000208-0000-4000-8000-000000000004";

const PLANTED_UPDATED_AT: &str = "2020-01-01T00:00:00.000Z";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn make_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("chat-scenario-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("chat-scenario-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
}

fn plant(base: &Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    let user_id: String = w
        .connection()
        .query_row("SELECT userId FROM chats LIMIT 1", [], |r| r.get(0))
        .expect("the committed fixture must carry at least one chat");
    for (id, summary, scenario) in [
        (SEEDED_A, SCENARIO, SCENARIO),
        (
            SEEDED_B,
            "A different scene entirely.",
            "A different scene entirely.",
        ),
        (
            REAL_SUMMARY,
            "# Scenario: Amy's Pool\n\nAmy is in her pool.\n\nThen they argued.",
            SCENARIO,
        ),
        (
            WHITESPACE_DIFFERS,
            "# Scenario: Amy's Pool\n\nAmy is in her pool.\n",
            SCENARIO,
        ),
    ] {
        w.connection()
            .execute(
                "INSERT INTO chats (id, userId, title, participants, \"contextSummary\", \
                  \"scenarioText\", \"messageCount\", \"createdAt\", \"updatedAt\") \
                 VALUES (?1, ?2, 'Planted', '[]', ?3, ?4, 0, ?5, ?5)",
                rusqlite::params![id, user_id, summary, scenario, PLANTED_UPDATED_AT],
            )
            .unwrap();
    }
}

fn quiet_config(base: &Path) -> HostConfig {
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
    config
}

async fn wait_until(mut probe: impl FnMut() -> bool, what: &str) {
    for _ in 0..400 {
        if probe() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn chat_row(db: &Db, id: &str) -> (Option<String>, Option<String>, String) {
    let id = id.to_string();
    db.read_main(move |c| {
        Ok(c.query_row(
            "SELECT \"contextSummary\", \"scenarioText\", \"updatedAt\" FROM chats WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    })
    .unwrap()
}

fn ledger(db: &Db) -> Option<(i64, String)> {
    db.read_main(|c| {
        Ok(c.query_row(
            "SELECT \"itemsAffected\", \"message\" FROM migrations_state WHERE id = ?1",
            rusqlite::params![MIGRATION_ID],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )
        .ok())
    })
    .unwrap()
}

/// The boot-repair thread runs the pass: both seeded chats are cleared, the real
/// summary and the whitespace-differing one survive, `updatedAt` moves on
/// EXACTLY the cleared rows, and the ledger row lands with v4's own sentence.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_clears_planted_scenario_seeded_summaries() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    plant(dir.path());

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();

    wait_until(
        || chat_row(&db, SEEDED_A).0.is_none(),
        "the boot heal to clear the first planted seed",
    )
    .await;

    let (summary_b, scenario_b, updated_b) = chat_row(&db, SEEDED_B);
    assert_eq!(summary_b, None, "the second seed must be cleared too");
    assert_eq!(
        scenario_b.as_deref(),
        Some("A different scene entirely."),
        "the scenario is never touched"
    );
    assert_ne!(
        updated_b, PLANTED_UPDATED_AT,
        "a cleared row's `updatedAt` must be bumped"
    );

    let (summary_real, _, updated_real) = chat_row(&db, REAL_SUMMARY);
    assert!(
        summary_real.is_some_and(|s| s.contains("Then they argued.")),
        "a real summary that quotes the scenario is not the seed"
    );
    assert_eq!(
        updated_real, PLANTED_UPDATED_AT,
        "an untouched row's `updatedAt` must NOT move — the bump rides the same predicate"
    );

    let (summary_ws, _, updated_ws) = chat_row(&db, WHITESPACE_DIFFERS);
    assert!(
        summary_ws.is_some_and(|s| s.ends_with('\n')),
        "byte equality: a trailing newline is not a match"
    );
    assert_eq!(updated_ws, PLANTED_UPDATED_AT);

    assert_eq!(
        ledger(&db),
        Some((
            2,
            "Cleared the scenario standing in as a summary on 2 conversations".to_string()
        )),
        "the pass must stamp its ledger row with v4's own sentence and count"
    );
}

/// A second boot is a no-op: the ledger row it wrote the first time stops it,
/// and a seed planted AFTER that first boot survives untouched. That is v4's
/// own runner semantics (`isMigrationCompleted` before `shouldRun`), and the
/// reason a clean boot must not stamp.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_second_boot_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    plant(dir.path());

    {
        let host = Host::start(quiet_config(dir.path())).unwrap();
        let db = host.core().db().unwrap();
        wait_until(
            || chat_row(&db, SEEDED_A).0.is_none(),
            "the first boot's heal",
        )
        .await;
        drop(db);
        drop(host);
    }

    // A seed that arrives after the pass has been recorded — an import from a
    // stale bundle is exactly how one would. The heal will not see it; the
    // ingest strip is what keeps it out, which is why both halves exist.
    const LATE_SEED: &str = "d0000208-0000-4000-8000-000000000005";
    {
        let w = Writer::open_writable(&dir.path().join("data/quilltap.db"), PEPPER).unwrap();
        let user_id: String = w
            .connection()
            .query_row("SELECT userId FROM chats LIMIT 1", [], |r| r.get(0))
            .unwrap();
        w.connection()
            .execute(
                "INSERT INTO chats (id, userId, title, participants, \"contextSummary\", \
                  \"scenarioText\", \"messageCount\", \"createdAt\", \"updatedAt\") \
                 VALUES (?1, ?2, 'Late', '[]', ?3, ?3, 0, ?4, ?4)",
                rusqlite::params![LATE_SEED, user_id, SCENARIO, PLANTED_UPDATED_AT],
            )
            .unwrap();
    }

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();
    // Give the boot-repair thread the same window the first test allows, then
    // require that nothing moved.
    wait_until(|| ledger(&db).is_some(), "the ledger row to be readable").await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let (summary, _, updated) = chat_row(&db, LATE_SEED);
    assert_eq!(
        summary.as_deref(),
        Some(SCENARIO),
        "the second boot must not run: the ledger row already claims the pass"
    );
    assert_eq!(updated, PLANTED_UPDATED_AT);
    assert_eq!(
        ledger(&db).map(|(n, _)| n),
        Some(2),
        "the ledger row must still record the FIRST boot's count"
    );
}

/// An instance with nothing to clear writes NO ledger row. v4's `shouldRun()`
/// counts the seeded rows, so its runner never records the migration on a clean
/// instance — and a stamp here would make a later v4 boot skip a migration it
/// never ran.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_clean_boot_stamps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    // No plant: the committed pair carries no seeded row.

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();
    // The boot-repair thread has no observable effect to wait on here, which is
    // the point — wait for a SIBLING pass's ledger row, so the window is proven
    // to have been long enough for this one to have stamped had it wanted to.
    wait_until(
        || {
            db.read_main(|c| {
                Ok(
                    c.query_row("SELECT COUNT(*) FROM migrations_state", [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap_or(-1),
                )
            })
            .unwrap()
                >= 0
        },
        "the boot-repair thread to reach the ledger",
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    assert_eq!(
        ledger(&db),
        None,
        "a boot that clears nothing must record nothing"
    );
}
