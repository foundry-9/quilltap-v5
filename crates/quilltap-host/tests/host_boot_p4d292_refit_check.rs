//! P4.D292 — v4's PHASE 3.8 (`void checkBuiltinRefit()`, `instrumentation.ts:
//! 995-1013`) and the refit debounce driver through the REAL `Host::start`.
//!
//! A fresh `setup` instance's default embedding profile is the BUILTIN one,
//! unfitted. With memories planted, the first boot's check is DUE (`unfitted`)
//! and enqueues ONE `EMBEDDING_REFIT` — fire-and-forget: `Host::start` returns
//! without waiting on it (the row lands after). Then a `schedule_refit` on the
//! live host is fired by the host's driver ~5 s later (a `source=scheduleRefit`
//! policy line, or the scheduler's own INFO), proving the cadence runs without
//! any caller pumping it.
//!
//! Lines are emitted from spawned host tasks, so this binary installs the
//! shared `CaptureLayer` as the process-GLOBAL default and holds ONE test.
//!
//! Mutation: delete the PHASE 3.8 spawn in `assemble` → no refit row and no
//! `source=check` line (red); delete the driver spawn → the debounce never
//! fires (red).
//!
//! Run:
//!   cargo test -p quilltap-host --test host_boot_p4d292_refit_check

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";
const SINGLE_USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

fn hermetic_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

async fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phase_three_eight_enqueues_a_due_refit_and_the_driver_fires_the_debounce() {
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
    )
    .expect("this binary owns the global subscriber");

    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
        let provider: String = w
            .connection()
            .query_row(
                "SELECT provider FROM embedding_profiles WHERE isDefault = 1",
                [],
                |r| r.get(0),
            )
            .expect("a fresh instance carries a default embedding profile");
        assert_eq!(
            provider, "BUILTIN",
            "the fixture premise: a BUILTIN default"
        );
        w.connection()
            .execute_batch(
                r#"INSERT INTO "memories" ("id","characterId","content","summary","createdAt","updatedAt") VALUES
                 ('e2920000-0000-4000-8000-000000000a01','e2920000-0000-4000-8000-0000000000c1','c1','s1','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z'),
                 ('e2920000-0000-4000-8000-000000000a02','e2920000-0000-4000-8000-0000000000c1','c2','s2','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')"#,
            )
            .unwrap();
    }

    let host = Host::start(hermetic_config(dir.path())).expect("the boot");
    let db = host.core().db().expect("a ready engine");

    // PHASE 3.8 — one refit, reason `unfitted`, source `check`.
    let refits = |db: &quilltap_core::db::runtime::Db| -> i64 {
        db.read_main(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM background_jobs WHERE type = 'EMBEDDING_REFIT'",
                [],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap()
    };
    wait_for("the PHASE 3.8 refit row", || refits(&db) >= 1).await;
    let has = |needle: &str| logs.lock().unwrap().iter().any(|l| l.contains(needle));
    wait_for("the PHASE 3.8 enqueue line", || {
        has("TF-IDF vocabulary refit enqueued module=embedding:builtin-refit-policy")
            && has("reason=unfitted source=check")
    })
    .await;
    assert_eq!(refits(&db), 1, "ONE refit, not one per caller");

    // The debounce — armed now, fired by the host's driver ≥ 5 s later.
    let armed = quilltap_core::clock::now_unix_ms();
    quilltap_core::services::embedding_job_scheduler::schedule_refit(
        &db,
        SINGLE_USER,
        None,
        false,
        armed,
    )
    .await
    .expect("schedule_refit");
    assert!(has(
        "[EmbeddingScheduler] Refit check scheduled (debounced)"
    ));
    wait_for("the driver's fire", || {
        has("source=scheduleRefit") || has("[EmbeddingScheduler] Refit not needed")
    })
    .await;
    assert!(
        quilltap_core::clock::now_unix_ms() - armed >= 5000,
        "the check fired before its 5 s debounce"
    );
    assert_eq!(
        quilltap_core::services::embedding_job_scheduler::pending_count(),
        0,
        "the fired check left the table"
    );
    drop(host);
}
