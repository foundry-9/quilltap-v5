//! P4.D222 — the boot ORDER pin: v4's Phase 3.66 (the help reconcile,
//! `492771aff`) is awaited BEFORE Phase 3.7 (the embedding-dimension
//! reconcile). v4's *why*: 3.66 must finish its writes before 3.7 can enqueue a
//! reindex whose sync would otherwise race it.
//!
//! Both phases log from threads the boot spawns (the help reconcile's own
//! runtime thread, the writer), which a thread-scoped capture cannot see — so
//! this binary installs the shared `CaptureLayer` as the process-GLOBAL default
//! and holds ONE test, so no sibling boot can interleave its lines.
//!
//! Mutation: swap the two calls in `assemble` (or put the dimension reconcile
//! back inside `seed_built_ins`) → the order assert reds.
//!
//! Run:
//!   cargo test -p quilltap-host --test host_help_boot_order

use std::path::Path;
use std::sync::{Arc, Mutex};

use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_help_reconcile_runs_before_the_dimension_reconcile() {
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
    )
    .expect("this binary owns the global subscriber");

    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");

    let host = Host::start(hermetic_config(dir.path())).unwrap();
    drop(host);

    let lines = logs.lock().unwrap().clone();
    let position = |needle: &str| {
        let hits: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains(needle))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(hits.len(), 1, "exactly one {needle:?} line: {hits:?}");
        hits[0]
    };
    let help = position("[HelpDocSync] Help docs reconciled");
    // The provisioned instance's default profile is the builtin one, so the
    // dimension reconcile reports "… skipped reason=builtin-profile"; with a
    // real profile it reports "… complete". Either is Phase 3.7's line.
    let dims = position("Embedding dimension reconciliation");
    assert!(
        help < dims,
        "Phase 3.66 (help, line {help}) must precede Phase 3.7 (dimensions, line {dims})"
    );
    // And the reconcile is not the boot's first line — the seeds ran before it.
    assert!(help > 0);
}
