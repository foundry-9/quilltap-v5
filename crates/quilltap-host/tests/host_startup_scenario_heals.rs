//! P4.171 host tests for the scenario heals at boot.
//!
//! - **C-19 (wiring):** v4's PHASE 3.4 / 3.4b loops
//!   (`services::startup_scenarios`) actually RUN on the host's boot-repair
//!   thread. The loops' behaviour and lines are proven against v4 by
//!   `startup_scenarios_equivalence`; this proves the one call nobody notices
//!   going missing. Over a COPY of the committed `chat-scenario-{main,mount}.db`
//!   pair with a storeless project and a storeless group PLANTED (a test that
//!   planted nothing would pass against a deleted call): after boot each has
//!   an official store holding `Scenarios/`, and the group also `Knowledge/`.
//! - **C-8 (P4.D295 HANDOFF (1)):** the General heal's failure logs the
//!   location's L2 (`[ScenarioLocation] Failed to ensure Scenarios folder`)
//!   THEN the host's L37 (`Error ensuring general scenarios folder, continuing
//!   startup`), over the location's REAL failing ensure — a `BEFORE INSERT`
//!   trigger on `doc_mount_folders` added after the General store exists, so
//!   only the folder write fails. No fault-injection switch in production.

use std::path::{Path, PathBuf};

use quilltap_core::db::doc_mount_points::{CreateOptions, DmpCreate, DocMountPointsRepository};
use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_core::services::builtin_mounts::ensure_general_scenarios_folder;
use quilltap_core::test_support::captured_with;
use quilltap_host::host::warn_general_scenarios_failure;
use quilltap_host::{Host, HostConfig};
use rusqlite::Connection;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

const PLANTED_PROJECT: &str = "d0000171-0000-4000-8000-000000000001";
const PLANTED_GROUP: &str = "d0000171-0000-4000-8000-000000000002";
const PLANTED_AT: &str = "2026-10-27T00:00:00.000Z";

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
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    for (table, id, name) in [
        ("projects", PLANTED_PROJECT, "Planted Heal"),
        ("groups", PLANTED_GROUP, "Planted Guild"),
    ] {
        w.connection()
            .execute(
                &format!(
                    "INSERT INTO \"{table}\" (id, name, \"officialMountPointId\", \"createdAt\", \"updatedAt\") \
                     VALUES (?1, ?2, NULL, ?3, ?3)"
                ),
                rusqlite::params![id, name, PLANTED_AT],
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
    config.memory_maintenance_interval_ms = hour;
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

fn official(db: &Db, table: &'static str, id: &str) -> Option<String> {
    let id = id.to_string();
    db.read_main(move |c| {
        Ok(c.query_row(
            &format!("SELECT \"officialMountPointId\" FROM \"{table}\" WHERE id = ?1"),
            rusqlite::params![id],
            |r| r.get::<_, Option<String>>(0),
        )?)
    })
    .unwrap()
}

fn store(db: &Db, mount_point_id: &str) -> (String, Vec<String>) {
    let mp = mount_point_id.to_string();
    db.read_mount_index(move |c| {
        let name: String = c.query_row(
            "SELECT name FROM doc_mount_points WHERE id = ?1",
            rusqlite::params![mp],
            |r| r.get(0),
        )?;
        let mut stmt = c.prepare(
            "SELECT path FROM doc_mount_folders WHERE \"mountPointId\" = ?1 ORDER BY path",
        )?;
        let folders = stmt
            .query_map(rusqlite::params![mp], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok((name, folders))
    })
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_boot_heals_storeless_projects_and_groups() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());

    let host = Host::start(quiet_config(dir.path())).unwrap();
    let db = host.core().db().unwrap();

    wait_until(
        || {
            official(&db, "projects", PLANTED_PROJECT).is_some()
                && official(&db, "groups", PLANTED_GROUP).is_some()
        },
        "the boot's scenario heals to provision the planted owners",
    )
    .await;

    let project_mp = official(&db, "projects", PLANTED_PROJECT).unwrap();
    let (name, folders) = store(&db, &project_mp);
    assert_eq!(name, "Project Files: Planted Heal");
    assert!(
        folders.iter().any(|f| f == "Scenarios"),
        "PHASE 3.4 heals the project's Scenarios/: {folders:?}"
    );

    let group_mp = official(&db, "groups", PLANTED_GROUP).unwrap();
    let (name, folders) = store(&db, &group_mp);
    assert_eq!(name, "Group Files: Planted Guild");
    assert!(
        folders.iter().any(|f| f == "Scenarios") && folders.iter().any(|f| f == "Knowledge"),
        "PHASE 3.4b heals the group's Scenarios/ and Knowledge/: {folders:?}"
    );
}

/// The General store + pointer, then a trigger that fails ONLY the folder
/// insert — so the location's real `ensure` reaches its folder write and
/// fails there.
fn failing_general_pair() -> (Connection, Connection) {
    let main = Connection::open_in_memory().unwrap();
    main.execute_batch(
        "CREATE TABLE instance_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);\
         INSERT INTO instance_settings VALUES ('generalMountPointId', 'd0000171-0000-4000-8000-0000000000aa');",
    )
    .unwrap();
    let mount = Connection::open_in_memory().unwrap();
    let dump: serde_json::Value = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/provisioning/fresh_schema.json"
    ))
    .unwrap();
    for stmt in dump["mountIndex"].as_array().unwrap() {
        mount.execute_batch(stmt.as_str().unwrap()).unwrap();
    }
    DocMountPointsRepository::new(&mount)
        .create(
            &DmpCreate {
                name: "Quilltap General".into(),
                base_path: String::new(),
                mount_type: "database".into(),
                store_type: "documents".into(),
                include_patterns: Vec::new(),
                exclude_patterns: Vec::new(),
                enabled: true,
                last_scanned_at: None,
                scan_status: "idle".into(),
                last_scan_error: None,
                conversion_status: "idle".into(),
                conversion_error: None,
                file_count: 0.0,
                chunk_count: 0.0,
                total_size_bytes: 0.0,
            },
            &CreateOptions {
                id: "d0000171-0000-4000-8000-0000000000aa".into(),
                created_at: PLANTED_AT.into(),
                updated_at: PLANTED_AT.into(),
            },
        )
        .unwrap();
    mount
        .execute_batch(
            "CREATE TRIGGER planted_folder_failure BEFORE INSERT ON doc_mount_folders \
             BEGIN SELECT RAISE(ABORT, 'planted folder failure'); END;",
        )
        .unwrap();
    (main, mount)
}

#[test]
fn a_failed_general_heal_logs_l2_then_l37() {
    let (main, mount) = failing_general_pair();
    let ((), lines) = captured_with(|| {
        if let Err(e) = ensure_general_scenarios_folder(&main, &mount) {
            warn_general_scenarios_failure(&e);
        }
    });
    let warns: Vec<&String> = lines.iter().filter(|l| l.starts_with("WARN ")).collect();
    assert_eq!(
        warns,
        [
            "WARN quilltap_core::scenarios::location [ScenarioLocation] Failed to ensure Scenarios folder \
             mountPointId=d0000171-0000-4000-8000-0000000000aa error=planted folder failure",
            "WARN quilltap::boot Error ensuring general scenarios folder, continuing startup \
             context=instrumentation.register error=planted folder failure",
        ],
        "{lines:#?}"
    );
}
