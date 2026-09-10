//! P4.D175 integration test: bug 132's placeholder-clear pass actually RUNS on
//! the host's boot-repair thread.
//!
//! `generated_image_placeholder_heal_equivalence` proves the ported pass against
//! v4's real migration by CALLING it directly, so it cannot see whether the host
//! ever does — the same gap that made P4.82's registration test necessary, and
//! the class the §3 reviews keep catching (a wiring is one call, and one call is
//! what nobody notices going missing). Here it matters more than usual: the pass
//! is the fix's user-visible half. While the caption sits in the column,
//! `auto_describe_precheck` answers `already-described` and the vision tier is
//! unreachable, so a heal that never runs leaves bug 132 live for every image
//! already on disk.
//!
//! Runs over a COPY of the committed `files-{main,mount}.db` pair with the two
//! label shapes PLANTED — the pair carries none naturally, and a test that
//! planted nothing would pass against a deleted call.

use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MIGRATION_ID: &str = "clear-generated-image-placeholder-descriptions-v1";

const PLANTED_GENERATED: &str = "d0000175-0000-4000-8000-000000000001";
const PLANTED_UPLOADED: &str = "d0000175-0000-4000-8000-000000000002";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn make_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::copy(
        fixtures_dir().join("files-main.db"),
        data.join("quilltap.db"),
    )
    .unwrap();
    std::fs::copy(
        fixtures_dir().join("files-mount.db"),
        data.join("quilltap-mount-index.db"),
    )
    .unwrap();
}

/// Plant one GENERATED row carrying the story-background label and one UPLOADED
/// row carrying the same string. The pass must clear the first and leave the
/// second, so a heal with the `source` conjunct dropped fails here too.
fn plant(base: &Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    let existing: (String, String) = w
        .connection()
        .query_row("SELECT userId, sha256 FROM files LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the committed fixture must carry at least one files row");
    for (id, source) in [
        (PLANTED_GENERATED, "GENERATED"),
        (PLANTED_UPLOADED, "UPLOADED"),
    ] {
        w.connection()
            .execute(
                "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, \
                  category, source, description, linkedTo, tags, fileStatus, createdAt, updatedAt) \
                 VALUES (?1, ?2, ?3, 'planted.webp', 'image/webp', 12, 'IMAGE', ?4, \
                  'Story background for: A Planted Chat', '[]', '[]', 'ok', \
                  '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
                rusqlite::params![id, existing.0, existing.1, source],
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

fn description_of(db: &Db, id: &str) -> Option<String> {
    db.read_main(move |c| {
        Ok(c.query_row(
            "SELECT description FROM files WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get::<_, Option<String>>(0),
        )?)
    })
    .unwrap()
}

fn ledger_rows(db: &Db) -> i64 {
    db.read_main(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM migrations_state WHERE id = ?1",
            rusqlite::params![MIGRATION_ID],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0))
    })
    .unwrap()
}

/// The boot-repair thread runs the pass: the planted GENERATED label is
/// cleared, the UPLOADED one survives, and the ledger row lands.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_clears_planted_placeholder_descriptions() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    plant(dir.path());

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();

    wait_until(
        || description_of(&db, PLANTED_GENERATED).is_none(),
        "the boot heal to clear the planted GENERATED label",
    )
    .await;

    assert_eq!(
        description_of(&db, PLANTED_UPLOADED).as_deref(),
        Some("Story background for: A Planted Chat"),
        "the UPLOADED row must survive — the predicate's `source` conjunct"
    );
    assert_eq!(ledger_rows(&db), 1, "the pass must stamp its ledger row");
}

/// A pre-written ledger row — v4 having already run its own migration on the
/// shared instance — leaves the labels alone. The cross-app leg, in the
/// direction that actually happens.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_v4_written_ledger_row_stops_the_boot_heal() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    plant(dir.path());
    {
        let w = Writer::open_writable(&dir.path().join("data/quilltap.db"), PEPPER).unwrap();
        w.connection()
            .execute_batch(
                // BOTH tables, as v4's `migrations/state.ts` creates them —
                // planting only `migrations_state` leaves every heal's
                // `migrations_metadata` upsert to throw, which fails the boot
                // (found by this test's first run; recorded in the lane record
                // as a latent fragility the three sibling heals share, since
                // each gates its CREATE batch on `migrations_state` alone).
                "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\
                   \"id\" TEXT PRIMARY KEY, \"completedAt\" TEXT NOT NULL, \
                   \"quilltapVersion\" TEXT NOT NULL, \
                   \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT); \
                 CREATE TABLE IF NOT EXISTS \"migrations_metadata\" (\
                   \"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);",
            )
            .unwrap();
        w.connection()
            .execute(
                "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) \
                 VALUES (?1, '2026-09-09T00:00:00.000Z', '4.10.0', 0, 'written by v4')",
                rusqlite::params![MIGRATION_ID],
            )
            .unwrap();
    }

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();
    // Give the boot-repair thread the same window the positive test needs.
    wait_until(|| ledger_rows(&db) == 1, "the boot to settle").await;
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    assert_eq!(
        description_of(&db, PLANTED_GENERATED).as_deref(),
        Some("Story background for: A Planted Chat"),
        "an existing ledger row must stop the pass — the label is still there"
    );
    assert_eq!(ledger_rows(&db), 1, "and no second row is written");
}
