//! P4.D304 (C1-4) — the two 4.11 `chat_settings` boot ensures through the
//! REAL `Host::start`: v5's ONE boot INFO per added column, capture-pinned in
//! v4's registration order (triggers, then slash), with the silence legs.
//!
//! (a) A PRE-4.11 instance — a fresh provision with both columns DROPPED
//!     (SQLite 3.35+ `DROP COLUMN`, the web-edge test's idiom): ONE boot
//!     appends both at the END (the migrations' position) with v4's clauses,
//!     the existing settings row reads the DDL defaults (`0` / `1`), the two
//!     INFOs land in order, none of v4's migration lines appear, and nothing
//!     is stamped (UNSTAMPED, R-B).
//! (b) The SECOND boot logs neither.
//! (c) A fresh `setup` instance (the columns at cid 18 / 23 from the dump)
//!     logs neither on its first boot.
//!
//! The chain runs inside `Db::write` (the writer thread), so the capture is
//! the process-wide subscriber, as `host_boot_p4d288_ensures`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const EXPECTED: [&str; 2] = [
    "INFO quilltap::boot Fitted the custom-tool trigger toggle migrationId=add-custom-tool-triggers-field-v1 columnAdded=true",
    "INFO quilltap::boot Fitted the composer slash-command toggle migrationId=add-composer-slash-commands-field-v1 columnAdded=true",
];

const IDS: [&str; 2] = [
    "add-custom-tool-triggers-field-v1",
    "add-composer-slash-commands-field-v1",
];

/// v4's migration lines (NO-PORT) — none may appear.
const V4_LINES: [&str; 3] = [
    "Added customToolTriggers column to chat_settings table",
    "Added composerSlashCommands column to chat_settings table",
    "Adding column",
];

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
        .filter(|l| {
            IDS.iter()
                .any(|id| l.contains(&format!("migrationId={id}")))
        })
        .cloned()
        .collect()
}

/// `(cid, name, type, dflt_value)` for the two 4.11 columns, by cid.
fn columns(data: &Path) -> Vec<(i64, String, String, Option<String>)> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let mut stmt = w
        .connection()
        .prepare(
            "SELECT cid, name, type, dflt_value FROM pragma_table_info('chat_settings') \
             WHERE name IN ('customToolTriggers', 'composerSlashCommands') ORDER BY cid",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn column_count(data: &Path) -> i64 {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    w.connection()
        .query_row(
            "SELECT count(*) FROM pragma_table_info('chat_settings')",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

fn stamped(data: &Path) -> Vec<String> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    if c.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'",
        [],
        |_| Ok(()),
    )
    .is_err()
    {
        return Vec::new();
    }
    let mut stmt = c.prepare("SELECT id FROM migrations_state").unwrap();
    let ids = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .filter(|id| IDS.contains(&id.as_str()))
        .collect();
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_4_11_instance_gains_both_columns_in_v4s_order() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        w.connection()
            .execute_batch(
                "ALTER TABLE \"chat_settings\" DROP COLUMN \"customToolTriggers\"; \
                 ALTER TABLE \"chat_settings\" DROP COLUMN \"composerSlashCommands\";",
            )
            .unwrap();
    }
    assert!(columns(&data).is_empty(), "the plant took both away");

    let lines = boot(dir.path());
    assert_eq!(ours(&lines), EXPECTED, "{lines:#?}");
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
    // Appended LAST (the migrations' position), triggers first, v4's clauses.
    assert_eq!(column_count(&data), 39);
    assert_eq!(
        columns(&data),
        [
            (
                37,
                "customToolTriggers".to_string(),
                "INTEGER".to_string(),
                Some("0".to_string())
            ),
            (
                38,
                "composerSlashCommands".to_string(),
                "INTEGER".to_string(),
                Some("1".to_string())
            ),
        ]
    );
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        let cells: (i64, i64) = w
            .connection()
            .query_row(
                "SELECT customToolTriggers, composerSlashCommands FROM chat_settings",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(cells, (0, 1), "the seeded row reads the DDL defaults");
    }
    assert!(stamped(&data).is_empty(), "UNSTAMPED (R-B)");

    // (b) the second boot: both no-ops, silent.
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert!(stamped(&data).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_boots_silent_with_the_columns_at_schema_positions() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert_eq!(
        columns(&data)
            .into_iter()
            .map(|(cid, name, _, _)| (cid, name))
            .collect::<Vec<_>>(),
        [
            (18, "customToolTriggers".to_string()),
            (23, "composerSlashCommands".to_string()),
        ]
    );
    assert!(stamped(&data).is_empty());
}
