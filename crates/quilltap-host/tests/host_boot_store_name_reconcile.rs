//! P4.D274 unit 7 — the boot store-name reconcile (v4 `ed8b15b50`, bug 186,
//! `instrumentation.ts:635-688`) and R-B's built-in re-provision, through the
//! REAL `Host::start`.
//!
//! (a) A planted unlinked vault-shaped store is RETIRED by the first boot,
//!     exactly once; the second boot logs only `planned … renameCount=0`.
//! (b) A DEGRADED mount index (a garbage sibling, P4.159) skips the whole
//!     mount-aware block, but the boot reconcile still logs v4's skip WARN.
//! (c) A planted phase-2 failure (a Zod-dropped row holding the name the
//!     retiree wants) logs v4's chain WARN, and the head-and-shoulders enqueue
//!     — the rest of v4's `.then` chain — does NOT run (its flag stays unset);
//!     the control boot (no plant) sets it.
//! (d) R-B (`BUILTIN_REPROVISION_FREE_NAME`): a dangling General pointer + a
//!     store squatting `quilltap general` → the boot succeeds and re-provisions
//!     `Quilltap General (2)`.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";
const MOUNT: &str = "quilltap-mount-index.db";

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

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

fn exec(path: &Path, sql: &str) {
    let w = Writer::open_writable(path, PEPPER).unwrap();
    w.connection()
        .execute_batch(sql)
        .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
}

fn query(path: &Path, sql: &str) -> Vec<String> {
    let w = Writer::open_writable(path, PEPPER).unwrap();
    let rows = w
        .connection()
        .prepare(sql)
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn boot(base: &Path) -> Vec<String> {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    lines
}

fn reconcile_lines(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|l| {
            l.contains("Store-name reconcile")
                || l.contains("Renamed document store to its rightful name")
                || l.contains("Error during character vault backfill or physical-file migration")
        })
        .collect()
}

fn fresh(base: &Path) -> std::path::PathBuf {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    data
}

const ORPHAN: &str = "5a000000-0000-4000-8000-0000000000f1";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_planted_orphan_vault_is_retired_once() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = fresh(dir.path());
    exec(
        &data.join(MOUNT),
        &format!(
            "INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) \
             VALUES ('{ORPHAN}', 'Tess Character Vault', 'character', \
                     '2026-02-02T02:02:02.000Z', '2026-02-02T02:02:02.000Z')"
        ),
    );

    let lines = boot(dir.path());
    let ours = reconcile_lines(&lines);
    assert_eq!(ours.len(), 2, "{ours:#?}");
    assert!(ours[0].starts_with(
        "DEBUG quilltap::mount_index Store-name reconcile planned module=mount-index:reconcile-store-names trigger=boot"
    ));
    assert!(ours[0].ends_with("renameCount=1"), "{}", ours[0]);
    assert_eq!(
        ours[1],
        &format!(
            "INFO quilltap::mount_index Renamed document store to its rightful name \
             module=mount-index:reconcile-store-names trigger=boot id={ORPHAN} \
             from=Tess Character Vault to=Tess Version 2026-02-02T020202Z Store reason=retired-vault"
        )
    );
    assert_eq!(
        query(
            &data.join(MOUNT),
            &format!("SELECT name FROM doc_mount_points WHERE id = '{ORPHAN}'")
        ),
        ["Tess Version 2026-02-02T020202Z Store"]
    );

    // The second boot: the plan is empty — `planned` only.
    let lines = boot(dir.path());
    let ours = reconcile_lines(&lines);
    assert_eq!(ours.len(), 1, "{ours:#?}");
    assert!(ours[0].ends_with("renameCount=0"), "{}", ours[0]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_degraded_mount_index_logs_v4s_skip_warn() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = fresh(dir.path());
    let garbage: Vec<u8> = (0..4608usize)
        .map(|i| ((i * 131 + 17) & 255) as u8)
        .collect();
    let swap = data.join(MOUNT).with_extension("swap");
    std::fs::write(&swap, garbage).unwrap();
    std::fs::rename(&swap, data.join(MOUNT)).unwrap();

    let lines = boot(dir.path());
    assert_eq!(
        reconcile_lines(&lines),
        ["WARN quilltap::mount_index Store-name reconcile skipped: mount-index database unavailable module=mount-index:reconcile-store-names trigger=boot"],
        "{lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_boot_reconcile_logs_the_chain_warn_and_skips_the_enqueue() {
    let _serial = SERIAL.lock().await;
    let flag = |data: &Path| {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        quilltap_core::services::headshoulders_backfill_enqueue::flag_value(w.connection())
    };

    // The control: a clean boot runs the enqueue, which sets its flag.
    let control = tempfile::tempdir().unwrap();
    let data = fresh(control.path());
    boot(control.path());
    assert!(flag(&data).is_some(), "the control boot runs the enqueue");

    // The plant: a Zod-dropped row (non-uuid id) holds the name the orphan's
    // retirement wants, so the two-phase apply fails on the unique index.
    let dir = tempfile::tempdir().unwrap();
    let data = fresh(dir.path());
    exec(
        &data.join(MOUNT),
        &format!(
            "INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) \
             VALUES ('{ORPHAN}', 'Nell Character Vault', 'character', \
                     '2026-02-02T02:02:02.000Z', '2026-02-02T02:02:02.000Z'); \
             INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) \
             VALUES ('not-a-uuid-nell', 'Nell Version 2026-02-02T020202Z Store', 'documents', \
                     '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')"
        ),
    );
    let lines = boot(dir.path());
    assert!(
        lines.iter().any(|l| l
            == "WARN quilltap_host::host Error during character vault backfill or physical-file migration context=instrumentation.register error=UNIQUE constraint failed: doc_mount_points.name"),
        "{:#?}",
        reconcile_lines(&lines)
    );
    assert_eq!(
        flag(&data),
        None,
        "the rest of v4's chain (the enqueue) is skipped"
    );
    assert_eq!(
        query(
            &data.join(MOUNT),
            &format!("SELECT name FROM doc_mount_points WHERE id = '{ORPHAN}'")
        ),
        ["Nell Character Vault"],
        "the savepoint rolled back"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn builtin_reprovision_free_name_at_boot() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = fresh(dir.path());
    let pointer = query(
        &data.join(MAIN),
        "SELECT value FROM instance_settings WHERE key = 'generalMountPointId'",
    );
    let general = pointer[0].trim_matches('"').to_string();
    exec(
        &data.join(MOUNT),
        &format!(
            "DELETE FROM doc_mount_points WHERE id = '{general}'; \
             INSERT INTO doc_mount_points (id, name, storeType, createdAt, updatedAt) \
             VALUES ('5a000000-0000-4000-8000-0000000000f2', 'quilltap general', 'documents', \
                     '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')"
        ),
    );
    boot(dir.path());
    let names = query(
        &data.join(MOUNT),
        "SELECT name FROM doc_mount_points WHERE name LIKE '%general%' ORDER BY name",
    );
    assert_eq!(names, ["Quilltap General (2)", "quilltap general"]);
}
