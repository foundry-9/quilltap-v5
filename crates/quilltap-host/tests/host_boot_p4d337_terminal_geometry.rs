//! P4.D337 (R-1) — the terminal-geometry boot ensure through the REAL
//! `Host::start`: v5's ONE boot INFO when it added the columns, capture-pinned,
//! with the silence legs (the P4.D324 shape).
//!
//! (a) A PRE-`a6292f169` instance — a fresh provision with `terminal_sessions`
//!     rebuilt WITHOUT `cols` / `rows` and holding one row: ONE boot appends
//!     both at the END with v4's MIGRATION clause (`INTEGER DEFAULT NULL`),
//!     ONE INFO (`columnsAdded=2`) lands, none of v4's migration lines appear,
//!     nothing is stamped (UNSTAMPED), and the old row reads NULL / NULL.
//! (b) The SECOND boot logs nothing.
//! (c) A fresh `setup` instance (generateDDL's `REAL` columns at their dump
//!     positions) logs nothing on its first boot and its `sqlite_master` is
//!     byte-unchanged.
//!
//! The ensure runs inside `Db::write` (the writer thread), so the capture is
//! the process-wide subscriber, as `host_boot_p4d324_inventory_columns`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const ID: &str = "add-terminal-session-geometry-v1";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const EXPECTED: &str = "INFO quilltap::boot Measured each terminal session's window for transcript replay migrationId=add-terminal-session-geometry-v1 columnsAdded=2";

/// v4's migration lines (NO-PORT — the deferred runner's) — none may appear.
const V4_LINES: [&str; 4] = [
    "Added geometry columns to terminal_sessions",
    "Failed to add geometry columns to terminal_sessions",
    "Added cols and rows columns to terminal_sessions",
    "Adding column",
];

/// The pre-`a6292f169` table, as generateDDL rendered it.
const LEGACY_DDL: &str = r#"CREATE TABLE "terminal_sessions" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "chatId" TEXT NOT NULL,
  "label" TEXT,
  "shell" TEXT NOT NULL,
  "cwd" TEXT NOT NULL,
  "startedAt" TEXT NOT NULL,
  "exitedAt" TEXT,
  "exitCode" REAL,
  "transcriptPath" TEXT,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
)"#;

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

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config.set_display_zone(quilltap_core::host_zone::TimeZone::UTC);
    config
}

fn boot(base: &Path) -> Vec<String> {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    lines
}

fn ours(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains(&format!("migrationId={ID}")))
        .cloned()
        .collect()
}

fn table_sql(data: &Path) -> String {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    w.connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'terminal_sessions'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn master(data: &Path) -> Vec<(String, Option<String>)> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare("SELECT name, sql FROM sqlite_master ORDER BY type, name")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn stamped(data: &Path) -> bool {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    c.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'",
        [],
        |_| Ok(()),
    )
    .is_ok()
        && c.query_row("SELECT 1 FROM migrations_state WHERE id = ?1", [ID], |_| {
            Ok(())
        })
        .is_ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_geometry_instance_gains_both_columns_with_one_info() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        w.connection()
            .execute_batch(&format!(
                "DROP TABLE \"terminal_sessions\"; {LEGACY_DDL}; \
                 INSERT INTO terminal_sessions (id, chatId, shell, cwd, startedAt, exitedAt, \
                   exitCode, createdAt, updatedAt) VALUES ('old', 'c1', '/bin/sh', '/', \
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:01:00.000Z', 0, \
                   '2026-01-01T00:00:00.000Z', '2026-01-01T00:01:00.000Z');"
            ))
            .unwrap();
    }
    assert!(
        !table_sql(&data).contains("\"cols\""),
        "the plant took them away"
    );

    let lines = boot(dir.path());
    assert_eq!(ours(&lines), [EXPECTED], "{lines:#?}");
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
    let sql = table_sql(&data);
    assert!(
        sql.ends_with(", \"cols\" INTEGER DEFAULT NULL, \"rows\" INTEGER DEFAULT NULL)"),
        "{sql}"
    );
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        let row = quilltap_core::db::terminal_sessions::find_by_id(w.connection(), "old")
            .unwrap()
            .expect("the old row");
        assert_eq!((row.cols, row.rows), (None, None), "no backfill");
    }
    assert!(!stamped(&data), "UNSTAMPED");

    // (b) the second boot: a no-op, silent.
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert_eq!(table_sql(&data), sql);
    assert!(!stamped(&data));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_boots_silent_with_the_real_columns_in_schema_order() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let before = master(&data);
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    let sql = table_sql(&data);
    assert!(
        sql.contains(
            "  \"transcriptPath\" TEXT,\n  \"cols\" REAL,\n  \"rows\" REAL,\n  \"createdAt\""
        ),
        "{sql}"
    );
    let after = master(&data);
    let ts = |m: &[(String, Option<String>)]| {
        m.iter()
            .filter(|(n, _)| n.contains("terminal_sessions"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(ts(&before), ts(&after), "sqlite_master byte-unchanged");
    assert!(!stamped(&data));
}
