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
    tx: broadcast::Sender<Event>,
    spawner: BusSpawner,
    now_ms: fn() -> i64,
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
        arm_realtime_bus_for_current_thread(tx.clone(), spawner.clone(), crate::clock::now_unix_ms);
        Self {
            tx,
            spawner,
            now_ms: crate::clock::now_unix_ms,
            rx,
            _guard: guard,
        }
    }

    /// Arm this capture's channel on `db`'s **writer thread** as well.
    ///
    /// The write pool is a dedicated OS thread (`db::runtime`'s
    /// `thread::Builder::spawn`), not a tokio task, so a publish made from
    /// inside a `db.write(…)` closure — bug 128's memory-gate twins live in the
    /// repository layer, which only ever runs there — is invisible to a bus
    /// armed on the test thread alone. Sending one no-op write job that arms
    /// the thread-local *there* is precise and race-free: that thread belongs
    /// to this test's own `Db` and dies with it, so there is nothing to disarm
    /// and nobody else to collect from. (The `BusSpawner` is already a captured
    /// `Handle` rather than bare `tokio::spawn` for exactly this reason — see
    /// [`HintCapture::start`].)
    pub async fn arm_writer_thread(&self, db: &crate::db::runtime::Db) {
        let tx = self.tx.clone();
        let spawner = self.spawner.clone();
        let now_ms = self.now_ms;
        db.write(move |_| {
            arm_realtime_bus_for_current_thread(tx, spawner, now_ms);
            Ok(())
        })
        .await
        .expect("arming the writer thread's bus");
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

    /// The collection POST's enqueue (v4 `POST /api/v1/system/jobs` →
    /// `enqueueJob`, a publish site). v5's `jobs_enqueue` writes the row at
    /// the API layer rather than through `queue_service::enqueue_job`, so it
    /// publishes there — the FOURTH enqueue site, found by the activated hint
    /// beat's first live run at the f3892158d-round unification.
    #[tokio::test]
    async fn the_collection_post_enqueue_publishes_too() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("enqpost");
        let resp = crate::api::system_data::jobs_enqueue_now(
            &db,
            "u1",
            "MEMORY_HOUSEKEEPING",
            &json!({}),
            None,
            None,
        )
        .await;
        assert!(
            matches!(resp, crate::api::types::Response::System(_)),
            "the enqueue itself must succeed for this pin to mean anything"
        );
        assert_eq!(cap.drain().await, vec![jobs()]);

        // …and the refusal arms write nothing, so they announce nothing.
        let refused = crate::api::system_data::jobs_enqueue_now(
            &db,
            "u1",
            "NOT_A_JOB_TYPE",
            &json!({}),
            None,
            None,
        )
        .await;
        assert!(!matches!(refused, crate::api::types::Response::System(_)));
        assert_eq!(cap.drain().await, vec![]);
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

// ── bug 128: the memory gate's two delete chokepoints ────────────────────────
//
// v4 (`4a9be9878`) publishes `memories` from `lib/memory/memory-gate.ts`'s
// `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch`, whose v5 twins are
// `db::memories::MemoriesRepository::{delete_with_unlink, delete_many_with_unlink}`.
// Publishing from the repository is what makes all EIGHT callers correct by
// construction, and it is also why these pins need
// [`HintCapture::arm_writer_thread`]: a repository method only ever runs on the
// write pool's dedicated OS thread.
//
// The route's NON-publish (`api::memories::memory_delete_by_chat`) is held two
// ways: behaviourally below — a chat with nothing to delete produces ZERO hints,
// which is the exact case v4's PR removed its own route publish for — and
// structurally by `realtime_publish_sites_guard`'s `api/memories.rs` row, whose
// expected count is 0.
#[cfg(test)]
mod memory_gate_tests {
    use super::*;
    use crate::db::memories::{CreateOptions, MemCreate};
    use crate::db::runtime::Db;
    use crate::db::Writer;
    use crate::realtime::types::RealtimeTopic;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const SENTINEL: &str = "2020-01-01T00:00:00.000Z";

    const DDL: &str = "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY, characterId TEXT, aboutCharacterId TEXT, chatId TEXT,
            projectId TEXT, content TEXT, summary TEXT, keywords TEXT, tags TEXT,
            importance REAL, embedding BLOB, source TEXT, witnessedContext TEXT,
            occurredAt TEXT, narrativeTime TEXT, entities TEXT DEFAULT '[]',
            kind TEXT DEFAULT 'semantic', sourceMessageId TEXT, lastAccessedAt TEXT,
            reinforcementCount REAL, lastReinforcedAt TEXT, relatedMemoryIds TEXT,
            reinforcedImportance REAL, createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_indices (
            id TEXT PRIMARY KEY, characterId TEXT, version REAL, dimensions REAL,
            createdAt TEXT, updatedAt TEXT);
        CREATE TABLE vector_entries (
            id TEXT PRIMARY KEY, characterId TEXT, embedding BLOB, createdAt TEXT);
    ";

    fn memories() -> (String, Option<String>) {
        (RealtimeTopic::Memories.as_str().to_string(), None)
    }

    /// A fresh encrypted DB seeded with `(id, characterId, chatId)` memories.
    fn make_db(tag: &str, rows: &[(&str, &str, &str)]) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(format!("{tag}.db"));
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection().execute_batch(DDL).unwrap();
            seed_memories(&w, rows);
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    /// Seed `(id, characterId, chatId)` memories into an open writer.
    fn seed_memories(w: &Writer, rows: &[(&str, &str, &str)]) {
        for (id, character_id, chat_id) in rows {
            w.memories()
                .create(
                    &MemCreate {
                        character_id: (*character_id).to_string(),
                        about_character_id: None,
                        chat_id: Some((*chat_id).to_string()),
                        project_id: None,
                        content: format!("content {id}"),
                        summary: format!("summary {id}"),
                        keywords: vec![],
                        tags: vec![],
                        importance: 0.5,
                        embedding: None,
                        source: "AUTO".to_string(),
                        witnessed_context: None,
                        occurred_at: None,
                        narrative_time: None,
                        entities: Vec::new(),
                        kind: "semantic".to_string(),
                        source_message_id: None,
                        last_accessed_at: None,
                        reinforcement_count: 1.0,
                        last_reinforced_at: None,
                        related_memory_ids: vec![],
                        reinforced_importance: 0.5,
                    },
                    &CreateOptions {
                        id: (*id).to_string(),
                        created_at: SENTINEL.to_string(),
                        updated_at: SENTINEL.to_string(),
                    },
                )
                .unwrap();
        }
    }

    #[tokio::test]
    async fn deleting_one_memory_announces_the_namespace() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("gate1", &[("m-1", "ch-1", "chat-1")]);
        cap.arm_writer_thread(&db).await;

        let deleted = db
            .write(|w| w.main().memories().delete_with_unlink("m-1"))
            .await
            .unwrap();
        assert!(
            deleted,
            "the delete must land for this pin to mean anything"
        );
        assert_eq!(cap.drain().await, vec![memories()]);
    }

    /// v4's `if (deleted)` guard: an already-gone memory is a no-op, and a
    /// no-op announces nothing.
    #[tokio::test]
    async fn deleting_an_already_gone_memory_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("gate2", &[("m-1", "ch-1", "chat-1")]);
        cap.arm_writer_thread(&db).await;

        let deleted = db
            .write(|w| w.main().memories().delete_with_unlink("m-nope"))
            .await
            .unwrap();
        assert!(!deleted);
        assert_eq!(cap.drain().await, vec![]);
    }

    /// The batch twin, and the coalescing that makes a cascade one hint.
    #[tokio::test]
    async fn a_batch_delete_announces_the_namespace_once() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db(
            "gate3",
            &[
                ("m-1", "ch-1", "chat-1"),
                ("m-2", "ch-1", "chat-1"),
                ("m-3", "ch-2", "chat-2"),
            ],
        );
        cap.arm_writer_thread(&db).await;

        let n = db
            .write(|w| {
                w.main().memories().delete_many_with_unlink(&[
                    "m-1".into(),
                    "m-2".into(),
                    "m-3".into(),
                ])
            })
            .await
            .unwrap();
        assert_eq!(n, 3);
        assert_eq!(cap.drain().await, vec![memories()]);
    }

    /// v4's `if (deleted > 0)`, both ways in: an EMPTY id list returns before
    /// the scan, and a list of ids that match nothing deletes nothing.
    #[tokio::test]
    async fn a_batch_that_deletes_nothing_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, db) = make_db("gate4", &[("m-1", "ch-1", "chat-1")]);
        cap.arm_writer_thread(&db).await;

        let empty = db
            .write(|w| w.main().memories().delete_many_with_unlink(&[]))
            .await
            .unwrap();
        assert_eq!(empty, 0);
        assert_eq!(cap.drain().await, vec![], "the empty-list early return");

        let missing = db
            .write(|w| {
                w.main()
                    .memories()
                    .delete_many_with_unlink(&["m-nope".into()])
            })
            .await
            .unwrap();
        assert_eq!(missing, 0);
        assert_eq!(
            cap.drain().await,
            vec![],
            "nothing matched, nothing changed"
        );
    }

    /// The REAL by-chat delete route, over a REAL provisioned main partition —
    /// the ownership check needs the whole `chats` table, so this seeds through
    /// `provision_fresh_instance` rather than a reduced hand-rolled DDL (which
    /// would also collide with the `chats` DDL this round is moving elsewhere).
    ///
    /// ONE collection-wide hint, published by the gate; the route contributes
    /// none of its own.
    async fn provisioned(tag: &str, rows: &[(&str, &str, &str)]) -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join(tag);
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let path = data.join("quilltap.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let mut chats: Vec<&str> = rows.iter().map(|(_, _, c)| *c).collect();
            chats.sort_unstable();
            chats.dedup();
            for chat in chats {
                w.connection()
                    .execute(
                        "INSERT INTO chats (id, userId, title, createdAt, updatedAt)                          VALUES (?1, 'u-1', 'T', ?2, ?2)",
                        rusqlite::params![chat, SENTINEL],
                    )
                    .unwrap();
            }
            seed_memories(&w, rows);
        }
        let db = Db::open_main(&path, PEPPER).unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn the_by_chat_delete_route_announces_once_from_the_gate() {
        let mut cap = HintCapture::start();
        let (_dir, db) = provisioned(
            "bychat",
            &[("m-1", "ch-1", "chat-1"), ("m-2", "ch-1", "chat-1")],
        )
        .await;
        cap.arm_writer_thread(&db).await;

        let resp = crate::api::memories::memory_delete_by_chat(&db, "chat-1").await;
        let crate::api::types::Response::Memory(body) = resp else {
            panic!("the route must succeed for this pin to mean anything");
        };
        assert_eq!(body["deletedCount"], 2);
        assert_eq!(cap.drain().await, vec![memories()]);
    }

    /// **Commit 2's exact edge** (v4 `ba89e0caa` removed the route's own publish
    /// for this): a chat with no memories returns before the gate is ever
    /// called, so nothing was deleted, nothing changed, and nothing is
    /// announced. A route-level publish would fire here.
    #[tokio::test]
    async fn the_route_announces_nothing_for_a_chat_with_no_memories() {
        let mut cap = HintCapture::start();
        let (_dir, db) = provisioned("bychatempty", &[("m-1", "ch-1", "chat-1")]).await;
        cap.arm_writer_thread(&db).await;
        db.write(|w| {
            w.main()
                .connection()
                .execute(
                    "INSERT INTO chats (id, userId, title, createdAt, updatedAt)                      VALUES ('chat-empty', 'u-1', 'T', '2020-01-01T00:00:00.000Z',                      '2020-01-01T00:00:00.000Z')",
                    [],
                )
                .map(|_| ())
                .map_err(Into::into)
        })
        .await
        .unwrap();

        let resp = crate::api::memories::memory_delete_by_chat(&db, "chat-empty").await;
        let crate::api::types::Response::Memory(body) = resp else {
            panic!("the route must succeed — a 404 would make this vacuous");
        };
        assert_eq!(body["deletedCount"], 0);
        assert_eq!(cap.drain().await, vec![]);
    }
}

// ===========================================================================
// P4.D183 — the message write funnel (v4 `5029075bb`)
// ===========================================================================

/// The transcript-change publish points: every write that changes what the
/// Salon renders announces the `chats` topic, scoped to the chat.
///
/// These are the ONLY thing in the tree that can see these hints. A hint is
/// not DB state, so the tier-2 funnel differential — which compares the
/// `chats` and `chat_messages` tables cell by cell — stays green with every
/// `publish_realtime` call deleted. The counter it bumps IS DB state and is
/// pinned there; the hint is pinned here.
///
/// Each conditional site is pinned in BOTH directions, because v4's conditions
/// are the interesting part and a guard with only its positive leg asserted is
/// a guard nothing tested (`a-guard-whose-other-conjuncts-are-false-is-untested`).
#[cfg(test)]
mod transcript_publish_sites {
    use super::*;
    use crate::db::chats_messages::ChatEventInput;
    use crate::db::chats_search::ChatSearchRepository;
    use crate::db::runtime::Db;
    use crate::db::Writer;
    use crate::realtime::types::RealtimeTopic;
    use serde_json::json;

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
    const T0: &str = "2020-01-01T00:00:00.000Z";

    fn chats(id: &str) -> (String, Option<String>) {
        (
            RealtimeTopic::Chats.as_str().to_string(),
            Some(id.to_string()),
        )
    }

    /// A REAL provisioned main partition plus P4.D182's boot ensure — the
    /// column arrives by ensure only (it is outside v4's `ChatMetadataSchema`,
    /// so `generateDDL` never emits it and `fresh_schema.json` cannot carry
    /// it: §R.5(a)). Without the ensure every bump here would take the
    /// swallowed-failure arm and the pins would still pass, measuring nothing
    /// about the counter.
    fn venue(tag: &str, chat_ids: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join(tag);
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let path = data.join("quilltap.db");
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            crate::db::chats_transcript_version_repair::ensure_chats_transcript_version_column(
                w.connection(),
            )
            .unwrap();
            for id in chat_ids {
                w.connection()
                    .execute(
                        "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
                         VALUES (?1, 'u-1', 'T', ?2, ?2)",
                        rusqlite::params![id, T0],
                    )
                    .unwrap();
            }
        }
        (dir, path)
    }

    fn msg(id: &str, content: &str) -> ChatEventInput {
        serde_json::from_value(json!({
            "type": "message",
            "id": id,
            "role": "USER",
            "content": content,
            "createdAt": T0,
        }))
        .unwrap()
    }

    /// The counter, read the way the chat GET reads it.
    fn version(path: &std::path::Path, chat_id: &str) -> i64 {
        let db = Db::open_main(path, PEPPER).unwrap();
        let id = chat_id.to_string();
        db.read_main(move |c| {
            Ok(crate::db::chats::ChatsRepository::new(c).get_transcript_version(&id))
        })
        .unwrap()
    }

    // ── add / add-batch: always, even with the chat row gone ────────────────

    #[tokio::test]
    async fn adding_a_message_announces_the_chat() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("add", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_message("chat-1", &msg("m-1", "hello"))
                .unwrap();
        }
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
        assert_eq!(version(&path, "chat-1"), 1, "the bump is the other half");
    }

    #[tokio::test]
    async fn adding_a_batch_announces_once() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("addb", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_messages("chat-1", &[msg("m-1", "a"), msg("m-2", "b")])
                .unwrap();
        }
        // ONE announce for the batch, not one per row — v4 commits once after
        // the loop. (The bus coalesces inside its window, so the COUNTER is the
        // discriminator that a per-row announce would move to 2.)
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
        assert_eq!(version(&path, "chat-1"), 1);
    }

    /// v4's `commitTranscriptChange` sits OUTSIDE the `if (chat)` that guards
    /// the bookkeeping: a message written into a chat whose row is already gone
    /// still changed a transcript, and still announces.
    #[tokio::test]
    async fn adding_to_a_missing_chat_row_still_announces() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("addmissing", &[]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_message("ghost", &msg("m-1", "hello"))
                .unwrap();
        }
        assert_eq!(cap.drain().await, vec![chats("ghost")]);
        // …and the bump found no row to move, exactly as v4's `updateOne`
        // returns `matchedCount: 0`.
        assert_eq!(version(&path, "ghost"), 0);
    }

    // ── update: after the row write, and NOT when the row is not there ──────

    #[tokio::test]
    async fn editing_a_message_announces_the_chat() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("upd", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let repo = w.chat_messages();
            repo.add_message("chat-1", &msg("m-1", "before")).unwrap();
            let _ = cap.drain().await; // the add's own hint
            assert!(repo
                .update_message("chat-1", "m-1", &json!({"content": "after"}))
                .unwrap());
        }
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
    }

    /// v4's test "says nothing when the message is not there" — the not-found
    /// early return is above the announce.
    #[tokio::test]
    async fn editing_a_message_that_is_not_there_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("updmissing", &["chat-1"]);
        let before;
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let repo = w.chat_messages();
            repo.add_message("chat-1", &msg("m-1", "x")).unwrap();
            let _ = cap.drain().await;
            before = version(&path, "chat-1");
            assert!(!repo
                .update_message("chat-1", "nope", &json!({"content": "y"}))
                .unwrap());
        }
        assert_eq!(cap.drain().await, vec![], "no row written, no hint");
        assert_eq!(version(&path, "chat-1"), before, "and no bump either");
    }

    // ── delete: only when something was removed ─────────────────────────────

    #[tokio::test]
    async fn deleting_a_message_announces_the_chat() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("del", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let repo = w.chat_messages();
            repo.add_messages("chat-1", &[msg("m-1", "a"), msg("m-2", "b")])
                .unwrap();
            let _ = cap.drain().await;
            assert_eq!(
                repo.delete_messages_by_ids("chat-1", &["m-1".to_string()])
                    .unwrap(),
                1
            );
        }
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
    }

    /// v4's test "says nothing when nothing was removed" — `if (removed > 0)`
    /// wraps the whole commit, not just the bookkeeping.
    #[tokio::test]
    async fn deleting_nothing_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("delnone", &["chat-1"]);
        let before;
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let repo = w.chat_messages();
            repo.add_message("chat-1", &msg("m-1", "a")).unwrap();
            let _ = cap.drain().await;
            before = version(&path, "chat-1");
            assert_eq!(
                repo.delete_messages_by_ids("chat-1", &["ghost".to_string()])
                    .unwrap(),
                0
            );
        }
        assert_eq!(cap.drain().await, vec![]);
        assert_eq!(version(&path, "chat-1"), before);
    }

    // ── clear: UNCONDITIONAL (the commit's one observable guard change) ──────

    #[tokio::test]
    async fn clearing_a_chat_announces_it() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("clr", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let repo = w.chat_messages();
            repo.add_message("chat-1", &msg("m-1", "a")).unwrap();
            let _ = cap.drain().await;
            assert!(repo.clear_messages("chat-1").unwrap());
        }
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
    }

    /// `5029075bb` lifted `clearMessages`'s commit out of its old `if (chat)`,
    /// so a clear on a chat with no row left announces anyway. This arm is the
    /// difference between the old shape and the new one.
    #[tokio::test]
    async fn clearing_a_chat_whose_row_is_gone_still_announces() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("clrmissing", &[]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            assert!(w.chat_messages().clear_messages("ghost").unwrap());
        }
        assert_eq!(cap.drain().await, vec![chats("ghost")]);
    }

    // ── search-and-replace: the one path outside the funnel ─────────────────

    #[tokio::test]
    async fn replacing_text_announces_the_chat() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("rep", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_message("chat-1", &msg("m-1", "hello world"))
                .unwrap();
            let _ = cap.drain().await;
            assert_eq!(
                ChatSearchRepository::new(w.connection())
                    .replace_in_messages("chat-1", "world", "there")
                    .unwrap(),
                1
            );
        }
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
    }

    /// The MEASURED condition (see `chats_search.rs`): v4's hunk looks
    /// unconditional, but `if (updatedCount === 0) return 0;` sits above the
    /// announce — so v4's test "says nothing when no message matched" is what
    /// actually ships.
    #[tokio::test]
    async fn replacing_nothing_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("repnone", &["chat-1"]);
        let before;
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_message("chat-1", &msg("m-1", "hello world"))
                .unwrap();
            let _ = cap.drain().await;
            before = version(&path, "chat-1");
            assert_eq!(
                ChatSearchRepository::new(w.connection())
                    .replace_in_messages("chat-1", "absent", "x")
                    .unwrap(),
                0
            );
        }
        assert_eq!(cap.drain().await, vec![]);
        assert_eq!(version(&path, "chat-1"), before);
    }

    // ── the counter's own contract ──────────────────────────────────────────

    /// "Bumps relative to the stored value, never to a snapshot it read": two
    /// announces land at +2 because `SET v = v + 1` reads and writes in one
    /// statement. A read-modify-write through the validated row rewrite could
    /// not promise this — which is why the column is outside
    /// `ChatMetadataSchema` and `$inc` is its only writer.
    #[tokio::test]
    async fn two_bumps_land_at_plus_two() {
        let _cap = HintCapture::start();
        let (_dir, path) = venue("inc", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            let repo = w.chat_messages();
            repo.announce_transcript_change("chat-1");
            repo.announce_transcript_change("chat-1");
        }
        assert_eq!(version(&path, "chat-1"), 2);
    }

    /// "Never writes the counter through a metadata patch": the bookkeeping
    /// `update()` this funnel performs cannot move the counter, because
    /// `ChatUpdate` has no field for it (P4.D182's source census is the other
    /// half of this guard; this is the behavioural leg).
    #[tokio::test]
    async fn a_metadata_patch_leaves_the_counter_alone() {
        let _cap = HintCapture::start();
        let (_dir, path) = venue("patch", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages().announce_transcript_change("chat-1");
            let patch = crate::db::chats::ChatUpdate {
                message_count: Some(7.0),
                title: Some("moved".into()),
                ..Default::default()
            };
            assert!(crate::db::chats::ChatsRepository::new(w.connection())
                .update("chat-1", &patch)
                .unwrap());
        }
        assert_eq!(version(&path, "chat-1"), 1, "the patch must not rewind it");
    }

    // ── the measured NEGATIVES: raw writers that are NOT announce sites ──────

    /// `ChatsRepository::delete` drops the chat's `chat_messages` rows with a
    /// raw `DELETE`, and announces NOTHING — v4 does not either. There is no
    /// transcript left to re-read, and a `chats` hint scoped to a row that no
    /// longer exists would send every open tab to a 404.
    ///
    /// The delete DOES publish through its own chokepoint elsewhere; what this
    /// pins is that the message sweep inside it does not reach the funnel's
    /// announce. Asserted as "no hint carrying THIS chat id" rather than "no
    /// hints at all", so a legitimate collection-level hint cannot make it red.
    #[tokio::test]
    async fn deleting_a_chat_does_not_announce_its_transcript() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("chatdel", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_message("chat-1", &msg("m-1", "a"))
                .unwrap();
            let _ = cap.drain().await;
            assert!(crate::db::chats::ChatsRepository::new(w.connection())
                .delete("chat-1")
                .unwrap());
        }
        let hints = cap.drain().await;
        assert!(
            !hints.contains(&chats("chat-1")),
            "deleting a chat must not announce its transcript; got {hints:?}"
        );
    }

    /// The daily maintenance sweep collapses stale caches — `compressionCache`,
    /// `renderedMarkdown`, and five discardable `chat_messages` columns — and
    /// announces NOTHING.
    ///
    /// MEASURED on v4 at `31436bae4`: `lib/background-jobs/maintenance/
    /// collapse-stale-chat-caches.ts` writes through `rawQuery`, bypassing the
    /// repository funnel entirely, and contains no `announceTranscriptChange`,
    /// no `publishRealtime` and no mention of the counter. That is deliberate
    /// on v4's part and not an oversight to "fix": the sweep discards derived
    /// data no reader depends on, and telling every open tab to re-read its
    /// whole transcript nightly would be the opposite of maintenance.
    #[tokio::test]
    async fn the_stale_cache_sweep_announces_nothing() {
        let mut cap = HintCapture::start();
        let (_dir, path) = venue("collapse", &["chat-1"]);
        {
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.chat_messages()
                .add_message("chat-1", &msg("m-1", "a"))
                .unwrap();
            // Give the sweep something to actually collapse, so the silence is
            // measured on a pass that DID write rather than one that no-opped.
            w.connection()
                .execute(
                    "UPDATE chats SET compressionCache = 'stale', updatedAt = ?1 WHERE id = 'chat-1'",
                    rusqlite::params![T0],
                )
                .unwrap();
        }
        let _ = cap.drain().await;
        let db = Db::open_main(&path, PEPPER).unwrap();
        let summary = crate::services::collapse_stale_chat_caches::collapse_stale_chat_caches(
            &db,
            crate::clock::now_unix_ms(),
        )
        .await
        .expect("the sweep must run for this pin to mean anything");
        assert!(
            summary.chat_rows_cleared > 0,
            "the sweep must have collapsed something, or the silence is vacuous: {summary:?}"
        );
        assert_eq!(cap.drain().await, vec![], "the sweep announces nothing");
    }

    /// A bump that cannot land — a pre-4.10 partition with no such column — is
    /// SWALLOWED (v4's `safeQuery(…, false)` slot) and the hint fires anyway.
    /// A tab told to look once too often is harmless; a tab never told is the
    /// bug the announce exists to prevent.
    #[tokio::test]
    async fn a_failed_bump_is_swallowed_and_the_hint_still_fires() {
        let mut cap = HintCapture::start();
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("nocol");
        std::fs::create_dir_all(&data).unwrap();
        crate::services::provisioning::provision_fresh_instance(&data, PEPPER).unwrap();
        let path = data.join("quilltap.db");
        {
            // Deliberately NO `ensure_chats_transcript_version_column` — this is
            // what a 4.9 instance looks like to a 4.10 binary before boot heals it.
            let w = Writer::open_writable(&path, PEPPER).unwrap();
            w.connection()
                .execute(
                    "INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
                     VALUES ('chat-1', 'u-1', 'T', ?1, ?1)",
                    rusqlite::params![T0],
                )
                .unwrap();
            w.chat_messages().announce_transcript_change("chat-1");
        }
        assert_eq!(cap.drain().await, vec![chats("chat-1")]);
        // …and the reader answers 0 rather than propagating the error.
        assert_eq!(version(&path, "chat-1"), 0);
    }
}
