//! P4.D324 (R-5) — the two Inventory column ensures through the REAL
//! `Host::start`: v5's ONE boot INFO per added column, capture-pinned in v4's
//! registration order (`chats.equippedInventory`, then
//! `characters.canManageInventory`), with the silence legs (the P4.D304
//! shape).
//!
//! (a) A PRE-INVENTORY instance — a fresh provision with both columns DROPPED:
//!     ONE boot appends both at the END (the migrations' position) with v4's
//!     MIGRATION clauses (`TEXT DEFAULT NULL` / `INTEGER DEFAULT NULL`), the
//!     two INFOs land in order, none of v4's migration lines appear, and
//!     nothing is stamped (UNSTAMPED).
//! (b) The SECOND boot logs neither.
//! (c) A fresh `setup` instance (the columns at their dump positions —
//!     `chats` cid 66, `characters` cid 34, generateDDL's bare types) logs
//!     neither on its first boot.
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
    "INFO quilltap::boot Fitted each chat with a ledger of what everyone is carrying migrationId=add-chat-equipped-inventory-v1 columnAdded=true",
    "INFO quilltap::boot Granted characters leave to rummage through their own bags migrationId=add-character-inventory-flag-v1 columnAdded=true",
];

const IDS: [&str; 2] = [
    "add-chat-equipped-inventory-v1",
    "add-character-inventory-flag-v1",
];

/// v4's migration lines (NO-PORT — the deferred runner's) — none may appear.
const V4_LINES: [&str; 3] = [
    "Added equippedInventory column to chats table",
    "Added canManageInventory column to characters table",
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

/// `(cid, type, dflt_value, column count)` for one table's column.
fn column(data: &Path, table: &str, name: &str) -> Option<(i64, String, Option<String>, i64)> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    let count: i64 = c
        .query_row(
            &format!("SELECT count(*) FROM pragma_table_info('{table}')"),
            [],
            |r| r.get(0),
        )
        .unwrap();
    c.query_row(
        &format!("SELECT cid, type, dflt_value FROM pragma_table_info('{table}') WHERE name = ?1"),
        [name],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, count)),
    )
    .ok()
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
async fn a_pre_inventory_instance_gains_both_columns_in_v4s_order() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        w.connection()
            .execute_batch(
                "ALTER TABLE \"chats\" DROP COLUMN \"equippedInventory\"; \
                 ALTER TABLE \"characters\" DROP COLUMN \"canManageInventory\";",
            )
            .unwrap();
    }
    assert!(
        column(&data, "chats", "equippedInventory").is_none(),
        "the plant took it away"
    );
    assert!(
        column(&data, "characters", "canManageInventory").is_none(),
        "the plant took it away"
    );

    let lines = boot(dir.path());
    assert_eq!(ours(&lines), EXPECTED, "{lines:#?}");
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
    // Appended LAST (the migrations' position), with v4's migration clauses.
    let (cid, ty, dflt, count) = column(&data, "chats", "equippedInventory").unwrap();
    assert_eq!(
        (cid, ty.as_str(), dflt.as_deref()),
        (count - 1, "TEXT", Some("NULL"))
    );
    let (cid, ty, dflt, count) = column(&data, "characters", "canManageInventory").unwrap();
    assert_eq!(
        (cid, ty.as_str(), dflt.as_deref()),
        (count - 1, "INTEGER", Some("NULL"))
    );
    assert!(stamped(&data).is_empty(), "UNSTAMPED");

    // (b) the second boot: both no-ops, silent.
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    assert!(stamped(&data).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_boots_silent_with_the_columns_at_dump_positions() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let lines = boot(dir.path());
    assert!(ours(&lines).is_empty(), "{lines:#?}");
    let (cid, ty, dflt, _) = column(&data, "chats", "equippedInventory").unwrap();
    assert_eq!((cid, ty.as_str(), dflt), (66, "TEXT", None));
    let (cid, ty, dflt, _) = column(&data, "characters", "canManageInventory").unwrap();
    assert_eq!((cid, ty.as_str(), dflt), (34, "INTEGER", None));
    assert!(stamped(&data).is_empty());
}
