//! The realtime publish-point census (P4.D124, v4 `f3892158d`).
//!
//! Every publish point is driven behaviourally by
//! `quilltap_core::realtime::publish_sites` and its siblings — with **one
//! exception this file exists for**: the enqueue-failure ROLLBACK inside
//! `begin_autonomous_run`. That publish can never be observed alone, because
//! the run-start patch's publish fires microseconds earlier on the same key and
//! the bus coalesces them — which is the bus working as designed, not a gap in
//! it. Dropping the rollback publish therefore changes nothing any test can
//! see.
//!
//! So the count is held here instead: `enclave/lifecycle.rs` must carry exactly
//! seven `autonomousRooms` publishes, one per v4 run-state transition. Six of
//! the seven also red a behavioural pin; this makes the seventh non-deletable
//! too, and makes an *added* eighth (a double-publish) visible.
//!
//! Run standalone (no oracle):
//!   cargo test -p quilltap-harness --test realtime_publish_sites_guard

use std::path::PathBuf;

/// `(repo-relative file, needle, expected count, why)`.
const CENSUS: &[(&str, &str, usize, &str)] = &[
    (
        "crates/quilltap-core/src/enclave/lifecycle.rs",
        "publish_realtime(RealtimeTopic::AutonomousRooms, None);",
        7,
        "v4's seven run-state transitions: `beginAutonomousRun`'s start patch AND \
         its enqueue-failure rollback (`autonomous-run-start.ts`), then pause, \
         stop, resume, updateAutonomousRoomSettings, and \
         reconcileFailedAutonomousTurn (`autonomous-room.service.ts`). The \
         rollback is the one with no isolatable behavioural pin — see this \
         file's header.",
    ),
    (
        "crates/quilltap-core/src/services/queue_service.rs",
        "publish_realtime(RealtimeTopic::Jobs, None);",
        5,
        "v4 has THREE queue-service publishes; v5 has five sites for them, \
         because v4's one `enqueueJob` is `enqueue_job` + \
         `enqueue_job_with_priority` + the blocking render enqueue's \
         in-transaction mint here. Plus the memory-extraction batch and the \
         if-it-took cancel.",
    ),
    (
        "crates/quilltap-core/src/services/job_runner.rs",
        "publish_realtime(RealtimeTopic::Jobs, None);",
        3,
        "the claim (PENDING→PROCESSING), the completion, and the failure — v4's \
         `job-dispatcher.ts` claim/markCompleted plus its THREE markFailed arms, \
         which collapse into v5's one (no child, in-process apply).",
    ),
    (
        "crates/quilltap-core/src/services/activity_registry.rs",
        "publish_realtime(RealtimeTopic::Jobs, None);",
        2,
        "both edges of an activity span. v4 also republishes from \
         `applyChildActivityDelta` / `resetChildActivity`, the child-mirror legs \
         v5 has no analogue for.",
    ),
    (
        "crates/quilltap-core/src/api/system_data.rs",
        "publish_realtime(RealtimeTopic::Jobs, None);",
        1,
        "the collection POST's enqueue. v4's route goes through `enqueueJob` — \
         a publish site — but v5's `jobs_enqueue` writes the row itself rather \
         than through `queue_service::enqueue_job`, so the hint is published at \
         the API layer. Found by the activated hint beat's FIRST live run at the \
         f3892158d-round unification: a fourth enqueue site neither lane's \
         survey table carried.",
    ),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn every_realtime_publish_site_is_present() {
    let root = repo_root();
    let mut failures: Vec<String> = Vec::new();

    for (file, needle, expected, why) in CENSUS {
        let path = root.join(file);
        let Ok(src) = std::fs::read_to_string(&path) else {
            failures.push(format!("{file}: unreadable (moved? renamed?)"));
            continue;
        };
        let found = src.matches(needle).count();
        if found != *expected {
            failures.push(format!(
                "{file}: {found} `{needle}` — expected {expected}.\n  {why}"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} publish-site census failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
