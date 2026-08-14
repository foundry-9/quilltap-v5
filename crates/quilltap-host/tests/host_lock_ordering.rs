//! P4.46 — the contended-start proof: the instance lock is claimed BEFORE any
//! partition is opened or created, on all three entrances (boot, unlock,
//! first-run setup).
//!
//! v4 locks inside `connect()`, ahead of `new Database(...)`. v5 acquired at
//! the head of the host assembler instead — i.e. after `Db::open` had opened
//! all three partitions writable (each issuing `PRAGMA journal_mode = TRUNCATE`,
//! a header write) and, on first-run setup, after the whole DDL replay and the
//! baseline seed had already been written. In the contended case that meant
//! writing to databases another live process believes it holds exclusively,
//! and only then refusing — the class v4's bug-58 fix closes, and the class the
//! CLAUDE.md open-sequence rule warns about (journal-mode writes racing the
//! cipher context).
//!
//! These tests plant a LIVE foreign lock and assert the refusal arrives with
//! the databases byte-for-byte untouched (and, for setup, with nothing created
//! at all). MUTATION-CHECKED on the lane: put acquisition back at the head of
//! `HostAssembler::assemble` and drop the `Setup` arm's `pre_open` call, and all
//! three contended tests go red — boot and unlock on "a refused boot wrote
//! bytes", setup on a `Setup` response where a refusal belongs (with a minted
//! pepper, a `.dbkey` and three partitions on disk).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use quilltap_core::api::{ErrorKind, PepperState, QuilltapCore as _, Request, Response};
use quilltap_core::db::Writer;
use quilltap_core::dbkey;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::lock::{self, LockFileContent};
use quilltap_host::{Host, HostConfig};

/// Any valid base64 keys a fresh encrypted DB; the same value opens it back.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// A real fresh instance (schema + baseline seed, all three partitions).
fn provisioned_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).unwrap();
}

/// Plant a lock held by a LIVE process that is not us.
///
/// PID 1 is alive on every Unix and is never this test process; `kill(1, 0)`
/// answers EPERM, which the port treats as alive exactly as v4 does. Same
/// hostname as us, so acquisition takes the "live, different process on the
/// same host" arm and refuses instead of reaping.
fn plant_contender_lock(base: &Path) {
    let content = LockFileContent {
        pid: 1,
        hostname: lock::hostname(),
        started_at: "2026-08-13T00:00:00.000Z".to_string(),
        last_heartbeat: "2026-08-13T00:00:00.000Z".to_string(),
        environment: quilltap_host::env::EnvironmentType::Local,
        process_title: "quilltap".to_string(),
        process_argv0: "/usr/local/bin/quilltap".to_string(),
        history: vec![],
    };
    let path = lock::instance_lock_path(base);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&content).unwrap()),
    )
    .unwrap();
}

/// Park every partition in WAL mode — which is what makes "did we open before
/// we locked?" observable at all.
///
/// Measured on this lane: `journal_mode` is a RUNTIME property for every mode
/// except WAL, so re-opening an already-TRUNCATE (or DELETE) partition rewrites
/// nothing and a byte comparison over it proves nothing. WAL is the one mode
/// SQLite persists (header bytes 18/19), so `Writer::open_writable`'s
/// `journal_mode = TRUNCATE` must checkpoint and rewrite the header to leave
/// it — an unlocked write to a database another live process believes it holds
/// exclusively. That is the class CLAUDE.md's open-sequence rule warns about
/// and bug 58's Ignite scenario in miniature.
fn park_partitions_in_wal(data: &Path) {
    for (name, _, _, _) in partition_state(data) {
        let w = Writer::open_writable(&data.join(name), PEPPER).unwrap();
        w.connection()
            .execute_batch("PRAGMA journal_mode = WAL;")
            .unwrap();
    }
}

/// The three partitions' content hash, length and mtime, sorted — the "not one
/// byte" comparand. Hashed rather than compared raw so a failure prints a line
/// instead of megabytes of ciphertext.
fn partition_state(data: &Path) -> Vec<(String, u64, u64, std::time::SystemTime)> {
    let mut out: Vec<(String, u64, u64, std::time::SystemTime)> = std::fs::read_dir(data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("db"))
        .map(|p| {
            let meta = std::fs::metadata(&p).unwrap();
            let bytes = std::fs::read(&p).unwrap();
            let mut hasher = DefaultHasher::new();
            bytes.hash(&mut hasher);
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                bytes.len() as u64,
                hasher.finish(),
                meta.modified().unwrap(),
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(out.len(), 3, "expected three partitions in {data:?}");
    out
}

fn dir_listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn base_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.env_pepper = None; // hermetic: ignore any ambient env pepper
    config
}

/// The happy control: an uncontended boot claims the lock (naming this PID)
/// before it serves, and `Lock` releases it — the semantics the reorder must
/// not have disturbed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_uncontended_boot_claims_and_releases_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    provisioned_instance(dir.path());
    let lock_path = lock::instance_lock_path(dir.path());
    assert!(!lock_path.exists(), "no lock before boot");

    let mut config = base_config(dir.path());
    config.env_pepper = Some(PEPPER.to_string());
    let host = Host::start(config).unwrap();

    let held: LockFileContent =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(held.pid, std::process::id(), "the boot claimed the lock");
    assert_eq!(held.hostname, lock::hostname());

    // Release stays exactly at shutdown.
    match host.core().dispatch(Request::Lock).await {
        Response::UnlockState(_) => {}
        other => panic!("unexpected: {other:?}"),
    }
    assert!(!lock_path.exists(), "shutdown released the lock");
}

/// Boot against a live foreign lock: refused, with the partitions untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn contended_boot_refuses_without_touching_the_databases() {
    let dir = tempfile::tempdir().unwrap();
    provisioned_instance(dir.path());
    let data = dir.path().join("data");
    plant_contender_lock(dir.path());
    park_partitions_in_wal(&data);
    let before = partition_state(&data);

    let mut config = base_config(dir.path());
    config.env_pepper = Some(PEPPER.to_string());
    let err = match Host::start(config) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("booted past a live foreign lock"),
    };
    assert!(err.contains("already using this database"), "{err}");
    assert!(err.contains("PID 1"), "{err}");
    assert_eq!(partition_state(&data), before, "a refused boot wrote bytes");
}

/// Unlock repeats the open sequence, so it needed the same reordering: a live
/// foreign lock refuses the unlock with the partitions untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn contended_unlock_refuses_without_touching_the_databases() {
    let dir = tempfile::tempdir().unwrap();
    provisioned_instance(dir.path());
    let data = dir.path().join("data");
    dbkey::save_dbkey(&data, PEPPER, "open sesame").unwrap();
    plant_contender_lock(dir.path());
    park_partitions_in_wal(&data);
    let before = partition_state(&data);

    // A locked boot opens nothing and therefore claims nothing — it succeeds
    // even while another instance holds the lock.
    let host = Host::start(base_config(dir.path())).unwrap();
    match host
        .core()
        .dispatch(Request::Unlock {
            passphrase: "open sesame".into(),
        })
        .await
    {
        Response::Error(e) => {
            assert_eq!(e.kind, ErrorKind::Internal);
            assert!(e.message.contains("already using this database"), "{e:?}");
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert_eq!(
        partition_state(&data),
        before,
        "a refused unlock wrote bytes"
    );
}

/// First-run setup was the worst of the three: three writable opens, the whole
/// DDL replay and the baseline seed all landed before any lock. Now a live
/// foreign lock stops it before the pepper is even minted — the data dir keeps
/// nothing but the contender's own lock file.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn contended_setup_creates_nothing_at_all() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    plant_contender_lock(dir.path());

    let host = Host::start(base_config(dir.path())).unwrap();
    match host.core().dispatch(Request::Health).await {
        Response::Health(h) => assert_eq!(h.pepper_state, PepperState::NeedsSetup),
        other => panic!("unexpected: {other:?}"),
    }

    match host
        .core()
        .dispatch(Request::Setup {
            passphrase: String::new(),
        })
        .await
    {
        Response::Error(e) => {
            assert_eq!(e.kind, ErrorKind::Internal);
            assert!(e.message.contains("already using this database"), "{e:?}");
        }
        other => panic!("unexpected: {other:?}"),
    }

    // No key file, no partitions: only the lock we planted.
    assert_eq!(dir_listing(&data), ["quilltap.lock"]);
}
