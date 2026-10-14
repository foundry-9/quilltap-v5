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
        7,
        "v4 has FOUR queue-service publishes (P4.D288: `enqueueEmbeddingGenerateMany`, \
         `af48f6cc2`, the fourth — v5's `enqueue_embedding_generate_many`); v5 has \
         seven sites for them, \
         because v4's one `enqueueJob` is `enqueue_job` + \
         `enqueue_job_with_priority` + TWO in-transaction mints that bypass \
         it (the blocking render enqueue, and P4.82's blocking \
         head-and-shoulders backfill enqueue, which the boot-repair write \
         closure calls while it already holds the writer). Plus the \
         memory-extraction batch and the if-it-took cancel.",
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
    (
        "crates/quilltap-core/src/db/memories.rs",
        "publish_realtime",
        0,
        "P4.D288 (C1-7, v4 `af48f6cc2`): the repository no longer publishes — \
         the gate's two delete chokepoints (bug 128's publish sites) were \
         deleted with v4's; every deletion is the ONE retire path, which \
         announces after the commit. A publish here would fire inside the \
         writer closure, before the rows are committed.",
    ),
    (
        "crates/quilltap-core/src/services/memory_retire.rs",
        "publish_realtime(RealtimeTopic::Memories, None);",
        1,
        "bug 128 re-homed (v4 `af48f6cc2`): `announceRetirement` publishes \
         `memories` once per committed retirement that deleted something \
         (R-AF — after the writer commit). ONE site, so every deletion caller \
         reaches the hint by construction; a SECOND would mean a caller \
         re-added its own.",
    ),
    (
        "crates/quilltap-core/src/services/admission/admit.rs",
        "publish_realtime(RealtimeTopic::Memories, None);",
        1,
        "P4.D290 (v4 `admit.ts:403`, `8af764514`): an admission whose outcome \
         has a row — insert, link, reinforce, absorb, promote — announces \
         `memories` once, on the caller's thread after its last write returned \
         (R-AF); a reject and a dry run write nothing and announce nothing. \
         This IS the manual create's hint (`api/memories.rs` stays at 0). The \
         behavioural half is `realtime::publish_sites::admission_publish_tests`.",
    ),
    (
        "crates/quilltap-core/src/services/admission/update_memory.rs",
        "publish_realtime(RealtimeTopic::Memories, None);",
        1,
        "P4.D290 (v4 `update-memory.ts:176`, `af48f6cc2`, bug 203): a hand edit \
         that wrote announces `memories` once, after the row and its link \
         deltas landed; a refused edit (not found, invalid related ids, an \
         embedding failure) writes nothing and announces nothing.",
    ),
    (
        "crates/quilltap-core/src/api/memories.rs",
        "publish_realtime",
        0,
        "bug 128's other half. v4's PR ADDED a publish to `handleDeleteByChatId` \
         in its first commit and REMOVED it in the second (`ba89e0caa`) as \
         redundant-and-wrong: the retirement already announces `memories` \
         collection-wide, which a chat-scoped subscriber takes (the client's id \
         filter only discards an event naming a DIFFERENT id), so a second hint \
         costs every subscriber a duplicate refetch — and would fire on the one \
         case the retirement is right to stay silent for, a chat with nothing to \
         delete. `git show 4a9be9878` is the NET, and the net is ZERO publishes \
         in this file. The behavioural half is \
         `realtime::publish_sites::memory_gate_tests`.",
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
