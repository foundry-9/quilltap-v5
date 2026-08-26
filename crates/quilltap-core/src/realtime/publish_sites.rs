//! The publish-point WIRING pins (P4.D124, v4 `f3892158d`).
//!
//! A hint is not DB state, so **no differential can see one**
//! (`differential-blind-to-a-log-only-fix.md`): every publish point could be
//! deleted and the whole workspace would stay green. These tests drive the REAL
//! entry points with a capturing subscriber on the engine's own broadcast
//! channel and assert the hints that came out — topic, id, and (where v4 is
//! conditional) that NO hint came out on the negative leg.
//!
//! ⚠ The capture arms a THREAD-SCOPED bus (`arm_realtime_bus_for_current_thread`)
//! rather than the process-global one. See that function's doc for why: a
//! globally-armed bus collects hints from every other test running concurrently
//! — and worse, makes a publish from a plain `#[test]` thread panic for want of
//! a reactor.
//!
//! Two sites are pinned elsewhere, beside their subjects, because the
//! scaffolding lives there: the seven autonomous run-state transitions in
//! `enclave::lifecycle`, and the post-commit write-batch hook in
//! `write_apply`.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::api::types::{Event, EventPayload};
use crate::realtime::bus::{
    arm_realtime_bus_for_current_thread, disarm_realtime_bus_for_current_thread, BusSpawner,
    COALESCE_WINDOW_MS,
};
use crate::realtime::types::RealtimeHint;

/// The bus is a process-global; its tests serialize on this.
static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Arms the bus onto a fresh channel, holds the serialization lock, and
/// disarms on drop.
pub struct HintCapture {
    rx: broadcast::Receiver<Event>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Drop for HintCapture {
    fn drop(&mut self) {
        disarm_realtime_bus_for_current_thread();
    }
}

impl HintCapture {
    /// Arm the bus and start capturing.
    pub fn start() -> Self {
        let guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        disarm_realtime_bus_for_current_thread();
        let (tx, rx) = broadcast::channel(256);
        // A captured Handle, not bare `tokio::spawn`: the flush may be queued
        // from the writer thread, which has no ambient runtime.
        let handle = tokio::runtime::Handle::current();
        let spawner: BusSpawner = Arc::new(move |fut| {
            handle.spawn(fut);
        });
        arm_realtime_bus_for_current_thread(tx, spawner, crate::clock::now_unix_ms);
        Self { rx, _guard: guard }
    }

    /// Let every coalescing window close, then drain the hints as
    /// `(topic, id)` pairs in arrival order.
    pub async fn drain(&mut self) -> Vec<(String, Option<String>)> {
        tokio::time::sleep(Duration::from_millis(COALESCE_WINDOW_MS + 20)).await;
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            if let EventPayload::Realtime(RealtimeHint { topic, id, .. }) = &ev.payload {
                out.push((topic.clone(), id.clone()));
            }
        }
        out
    }

    /// The drained hints, sorted — for sites whose several hints have no
    /// contractual order.
    pub async fn drain_sorted(&mut self) -> Vec<(String, Option<String>)> {
        let mut out = self.drain().await;
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::background_jobs::{BackgroundJobsRepository, BjCreate, CreateOptions};
    use crate::db::runtime::Db;
    use crate::realtime::types::RealtimeTopic;
    use crate::services::activity_kinds::ActivityKind;
    use crate::services::activity_registry::{begin_activity, track_activity, ActivityTestGuard};
    use crate::services::queue_service;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    const DDL: &str = "CREATE TABLE background_jobs (\
        id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
        payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
        maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
        startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

    fn make_db(tag: &str) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{tag}.db"));
        {
            let w = crate::db::Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    fn jobs() -> (String, Option<String>) {
        (RealtimeTopic::Jobs.as_str().to_string(), None)
    }

    // ── queue-service: enqueue ───────────────────────────────────────────────

    #[tokio::test]
    async fn enqueuing_a_job_publishes_the_jobs_topic() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("enq");
        queue_service::enqueue_job(&db, "u1", "MEMORY_HOUSEKEEPING", json!({}), 3.0)
            .await
            .unwrap();
        assert_eq!(cap.drain().await, vec![jobs()]);
    }

    /// v4 has ONE `enqueueJob`; v5 split it in two, so the priority variant is
    /// the same v4 site and must publish too.
    #[tokio::test]
    async fn enqueuing_with_a_priority_publishes_too() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("enqp");
        queue_service::enqueue_job_with_priority(
            &db,
            "u1",
            "CHAT_DANGER_CLASSIFICATION",
            json!({}),
            -1.0,
            3.0,
        )
        .await
        .unwrap();
        assert_eq!(cap.drain().await, vec![jobs()]);
    }

    /// A storm of enqueues is ONE hint — the whole reason the bus coalesces.
    #[tokio::test]
    async fn a_batch_of_enqueues_arrives_as_one_hint() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("enqmany");
        for _ in 0..12 {
            queue_service::enqueue_job(&db, "u1", "MEMORY_HOUSEKEEPING", json!({}), 3.0)
                .await
                .unwrap();
        }
        assert_eq!(cap.drain().await, vec![jobs()], "twelve enqueues, one hint");
    }

    /// The memory-extraction BATCH goes straight to `create_batch`, bypassing
    /// `enqueue_job` — so it needs (and v4 gives it) its own publish, under the
    /// same guard as its `ensureProcessorRunning`.
    #[tokio::test]
    async fn the_memory_extraction_batch_publishes() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("batch");
        let entries = vec![
            queue_service::MemoryExtractionBatchEntry {
                turn_opener_message_id: Some("m-1".into()),
                extraction_anchor_message_id: None,
            },
            queue_service::MemoryExtractionBatchEntry {
                turn_opener_message_id: Some("m-2".into()),
                extraction_anchor_message_id: None,
            },
        ];
        queue_service::enqueue_memory_extraction_batch(&db, "u1", "c-1", "cp-1", &entries, 0.0)
            .await
            .unwrap();
        assert_eq!(cap.drain().await, vec![jobs()], "two jobs, one hint");
    }

    /// …and an EMPTY batch publishes nothing: v4 guards both the publish and
    /// the processor kick on `jobIds.length > 0`.
    #[tokio::test]
    async fn an_empty_memory_extraction_batch_publishes_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("batch0");
        queue_service::enqueue_memory_extraction_batch(&db, "u1", "c-1", "cp-1", &[], 0.0)
            .await
            .unwrap();
        assert_eq!(cap.drain().await, vec![]);
    }

    /// The ASYNC render enqueue routes through `enqueue_job_with_priority`, so
    /// what this pins is the DEDUPE arm: a call that finds a pending render
    /// writes nothing and must therefore say nothing.
    #[tokio::test]
    async fn the_conversation_render_enqueue_publishes_only_when_it_creates() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("render");

        let (_id, is_new) =
            queue_service::enqueue_conversation_render(&db, "u1", "c-1", Some(true))
                .await
                .unwrap();
        assert!(is_new);
        assert_eq!(cap.drain().await, vec![jobs()]);

        // A second call for the same chat dedupes onto the pending row.
        let (_id2, is_new2) =
            queue_service::enqueue_conversation_render(&db, "u1", "c-1", Some(true))
                .await
                .unwrap();
        assert!(!is_new2, "the premise: this one deduped");
        assert_eq!(
            cap.drain().await,
            vec![],
            "a dedupe writes nothing and says nothing"
        );
    }

    /// The BLOCKING render enqueue is the third v5 site for v4's one
    /// `enqueueJob` publish: the boot reconcile runs inside one
    /// `db.write_blocking` closure, so it mints the row itself instead of
    /// awaiting `enqueue_job_with_priority`. Without its own publish it would be
    /// the one enqueue no client ever hears about.
    #[tokio::test]
    async fn the_blocking_render_enqueue_publishes_too() {
        let mut cap = HintCapture::start();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("boot.db");
        let w = crate::db::Writer::open_writable(&path, PEPPER).unwrap();
        w.connection().execute_batch(DDL).unwrap();

        let (_id, is_new) =
            queue_service::enqueue_conversation_render_blocking(w.connection(), "u1", "c-1", None)
                .unwrap();
        assert!(is_new);
        assert_eq!(cap.drain().await, vec![jobs()]);

        // …and its dedupe arm is silent, as the async one's is.
        let (_id2, is_new2) =
            queue_service::enqueue_conversation_render_blocking(w.connection(), "u1", "c-1", None)
                .unwrap();
        assert!(!is_new2);
        assert_eq!(cap.drain().await, vec![]);
    }

    // ── queue-service: cancel ────────────────────────────────────────────────

    #[tokio::test]
    async fn cancelling_publishes_only_when_the_cancel_took() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("cancel");
        let id = queue_service::enqueue_job(&db, "u1", "MEMORY_HOUSEKEEPING", json!({}), 3.0)
            .await
            .unwrap();
        // Drain the enqueue's own hint first.
        assert_eq!(cap.drain().await, vec![jobs()]);

        assert!(queue_service::cancel_job(&db, &id).await.unwrap());
        assert_eq!(cap.drain().await, vec![jobs()], "a cancel that took");

        // A second cancel of the same (now DEAD) job does not take…
        assert!(!queue_service::cancel_job(&db, &id).await.unwrap());
        assert_eq!(
            cap.drain().await,
            vec![],
            "a cancel that did not take is silent"
        );

        // …and neither does one for a job that never existed.
        assert!(!queue_service::cancel_job(&db, "no-such-job").await.unwrap());
        assert_eq!(cap.drain().await, vec![]);
    }

    // ── the activity registry's span edges ───────────────────────────────────

    #[tokio::test]
    async fn both_edges_of_an_activity_span_publish() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        // Begin and end are separated by a full coalescing window so the two
        // edges cannot collapse into one another.
        let span = begin_activity(ActivityKind::Image);
        assert_eq!(cap.drain().await, vec![jobs()], "the opening edge");
        span.end();
        assert_eq!(cap.drain().await, vec![jobs()], "the closing edge");
    }

    /// A span shorter than the window is still ONE hint — the coalescing that
    /// makes wrapping a hot chokepoint safe.
    #[tokio::test]
    async fn a_short_span_publishes_one_coalesced_hint() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        track_activity(ActivityKind::Danger, async {}).await;
        assert_eq!(cap.drain().await, vec![jobs()]);
    }

    // ── the job runner: claim, completion + entity hints, failure ────────────

    async fn seed_pending(db: &Db, id: &str, job_type: &str, payload: serde_json::Value) {
        let create = BjCreate {
            user_id: "u1".into(),
            job_type: job_type.into(),
            status: Some("PENDING".into()),
            payload,
            priority: 0.0,
            attempts: 0.0,
            max_attempts: 3.0,
            last_error: None,
            scheduled_at: "2020-01-01T00:00:00.000Z".into(),
            started_at: None,
            completed_at: None,
        };
        let opts = CreateOptions {
            id: id.into(),
            created_at: "2020-01-01T00:00:00.000Z".into(),
            updated_at: "2020-01-01T00:00:00.000Z".into(),
        };
        db.write(move |ws| {
            BackgroundJobsRepository::new(ws.main().connection()).create(&create, &opts)
        })
        .await
        .unwrap();
    }

    struct OkHandler;
    impl crate::services::job_runner::JobHandler for OkHandler {
        fn handle<'a>(
            &'a self,
            _db: &'a Db,
            _job: &'a crate::db::background_jobs::BackgroundJob,
        ) -> crate::services::job_runner::JobFuture<'a> {
            Box::pin(async { crate::services::job_runner::JobOutcome::Completed(None) })
        }
    }

    struct FailHandler;
    impl crate::services::job_runner::JobHandler for FailHandler {
        fn handle<'a>(
            &'a self,
            _db: &'a Db,
            _job: &'a crate::db::background_jobs::BackgroundJob,
        ) -> crate::services::job_runner::JobFuture<'a> {
            Box::pin(async { crate::services::job_runner::JobOutcome::Failed("boom".into()) })
        }
    }

    /// A completed job publishes `jobs` AND the entity hints its type + payload
    /// name. (The claim publishes `jobs` too; the two collapse in the window,
    /// which is exactly the intent — one `jobs` hint per pump.)
    #[tokio::test]
    async fn a_completed_job_publishes_jobs_and_its_entity_hints() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("complete");
        seed_pending(
            &db,
            "j1",
            "CHARACTER_AVATAR_GENERATION",
            json!({ "chatId": "c-1", "characterId": "ch-1" }),
        )
        .await;

        let mut reg = crate::services::job_runner::HandlerRegistry::new();
        reg.register("CHARACTER_AVATAR_GENERATION", Box::new(OkHandler));
        crate::services::job_runner::JobRunner::new(db.clone(), reg)
            .pump_claim()
            .await;

        assert_eq!(
            cap.drain_sorted().await,
            vec![
                ("characters".to_string(), Some("ch-1".to_string())),
                ("chats".to_string(), Some("c-1".to_string())),
                ("jobs".to_string(), None),
            ]
        );
    }

    /// A job type with no entity mapping still moves `jobs` — the queue changed.
    #[tokio::test]
    async fn a_completed_job_with_no_entity_topic_still_publishes_jobs() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("complete2");
        seed_pending(&db, "j1", "LLM_LOG_CLEANUP", json!({})).await;

        let mut reg = crate::services::job_runner::HandlerRegistry::new();
        reg.register("LLM_LOG_CLEANUP", Box::new(OkHandler));
        crate::services::job_runner::JobRunner::new(db.clone(), reg)
            .pump_claim()
            .await;

        assert_eq!(cap.drain_sorted().await, vec![jobs()]);
    }

    /// A FAILED job publishes `jobs` and NO entity hints — v4 publishes the
    /// completion hints only on the success arm.
    #[tokio::test]
    async fn a_failed_job_publishes_jobs_and_no_entity_hints() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("fail");
        seed_pending(&db, "j1", "TITLE_UPDATE", json!({ "chatId": "c-1" })).await;

        let mut reg = crate::services::job_runner::HandlerRegistry::new();
        reg.register("TITLE_UPDATE", Box::new(FailHandler));
        crate::services::job_runner::JobRunner::new(db.clone(), reg)
            .pump_claim()
            .await;

        assert_eq!(
            cap.drain_sorted().await,
            vec![jobs()],
            "a failure moves the queue but announces no entity"
        );
    }

    /// A handler that outlives a coalescing window, so the CLAIM's hint flushes
    /// on its own instead of collapsing into the terminal transition's.
    struct SlowHandler {
        succeed: bool,
    }
    impl crate::services::job_runner::JobHandler for SlowHandler {
        fn handle<'a>(
            &'a self,
            _db: &'a Db,
            _job: &'a crate::db::background_jobs::BackgroundJob,
        ) -> crate::services::job_runner::JobFuture<'a> {
            let succeed = self.succeed;
            Box::pin(async move {
                tokio::time::sleep(Duration::from_millis(COALESCE_WINDOW_MS + 60)).await;
                if succeed {
                    crate::services::job_runner::JobOutcome::Completed(None)
                } else {
                    crate::services::job_runner::JobOutcome::Failed("boom".into())
                }
            })
        }
    }

    /// ⚠ The claim's own hint is INVISIBLE in a fast pump — it coalesces with
    /// the completion's, so deleting it changes nothing observable. Separating
    /// the two windows is the only way to pin it, and this is what does that: a
    /// handler slower than the window, with the stream drained mid-flight.
    #[tokio::test]
    async fn the_claim_transition_publishes_on_its_own() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("claimhint");
        seed_pending(&db, "j1", "LLM_LOG_CLEANUP", json!({})).await;

        let mut reg = crate::services::job_runner::HandlerRegistry::new();
        reg.register("LLM_LOG_CLEANUP", Box::new(SlowHandler { succeed: true }));
        let runner = crate::services::job_runner::JobRunner::new(db.clone(), reg);
        let pump = tokio::spawn(async move { runner.pump_claim().await });

        // Drained while the handler is still running: only the claim can have
        // published by now.
        assert_eq!(cap.drain().await, vec![jobs()], "PENDING → PROCESSING");

        pump.await.unwrap();
        assert_eq!(cap.drain().await, vec![jobs()], "…then the completion");
    }

    /// The same separation for the FAILURE arm, which otherwise hides behind
    /// the claim's hint.
    #[tokio::test]
    async fn the_failure_transition_publishes_on_its_own() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("failhint");
        seed_pending(&db, "j1", "TITLE_UPDATE", json!({ "chatId": "c-1" })).await;

        let mut reg = crate::services::job_runner::HandlerRegistry::new();
        reg.register("TITLE_UPDATE", Box::new(SlowHandler { succeed: false }));
        let runner = crate::services::job_runner::JobRunner::new(db.clone(), reg);
        let pump = tokio::spawn(async move { runner.pump_claim().await });

        assert_eq!(cap.drain().await, vec![jobs()], "the claim");
        pump.await.unwrap();
        assert_eq!(
            cap.drain_sorted().await,
            vec![jobs()],
            "the failure moves the queue — and announces no entity"
        );
    }

    /// An EMPTY queue publishes nothing: v4 publishes on a successful claim,
    /// not on every pump.
    #[tokio::test]
    async fn pumping_an_empty_queue_publishes_nothing() {
        let _a = ActivityTestGuard::new();
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("emptypump");
        let reg = crate::services::job_runner::HandlerRegistry::new();
        crate::services::job_runner::JobRunner::new(db.clone(), reg)
            .pump_claim()
            .await;
        assert_eq!(cap.drain().await, vec![]);
    }
}
