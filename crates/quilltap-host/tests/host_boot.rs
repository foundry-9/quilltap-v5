//! P4.0's exit criterion (milestone M0): boot a fixture instance headless
//! through `quilltap-host` and pump enqueued jobs to completion — plus the
//! first dispatches (health / unlock family / instances / list-chats) and a
//! full lock → unlock → drivers-restart cycle against a `.dbkey`-protected
//! fixture.
//!
//! The fixture is a hand-rolled instance directory (`<base>/data/quilltap.db`
//! with the loose-typed table shapes the core's own runner/enclave self-tests
//! use — the tier-2/tier-3 differentials own real-schema fidelity; this test
//! owns the composition).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use quilltap_core::api::{ErrorKind, PepperState, QuilltapCore as _, Request, Response};
use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_core::dbkey;
use quilltap_core::services::job_runner::{JobFuture, JobHandler, JobOutcome};
use quilltap_core::services::queue_service;
use quilltap_host::{Host, HostConfig};

/// Any valid base64 keys a fresh encrypted DB; the same value opens it back.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

const BACKGROUND_JOBS_DDL: &str = "CREATE TABLE background_jobs (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
    payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
    maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
    startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

/// The full `chats` column set, loose types (the enclave self-tests' shape).
fn chats_ddl() -> String {
    let cols = [
        "id TEXT PRIMARY KEY",
        "userId TEXT",
        "participants TEXT",
        "title TEXT",
        "contextSummary TEXT",
        "sillyTavernMetadata TEXT",
        "tags TEXT",
        "roleplayTemplateId TEXT",
        "timestampConfig TEXT",
        "lastTurnParticipantId TEXT",
        "messageCount REAL",
        "lastMessageAt TEXT",
        "lastRenameCheckInterchange REAL",
        "compactionGeneration REAL",
        "lastSummaryTurn REAL",
        "lastSummaryTokens REAL",
        "lastFullRebuildTurn REAL",
        "summaryAnchorMessageIds TEXT",
        "isPaused INTEGER",
        "isManuallyRenamed INTEGER",
        "impersonatingParticipantIds TEXT",
        "activeTypingParticipantId TEXT",
        "allLLMPauseTurnCount REAL",
        "turnQueue TEXT",
        "spokenThisCycleParticipantIds TEXT",
        "documentEditingMode INTEGER",
        "documentMode TEXT",
        "dividerPosition REAL",
        "terminalMode TEXT",
        "activeTerminalSessionId TEXT",
        "rightPaneVerticalSplit REAL",
        "projectId TEXT",
        "scenarioText TEXT",
        "totalPromptTokens REAL",
        "totalCompletionTokens REAL",
        "estimatedCostUSD REAL",
        "priceSource TEXT",
        "showSystemEventsOverride INTEGER",
        "requestFullContextOnNextMessage INTEGER",
        "disabledTools TEXT",
        "disabledToolGroups TEXT",
        "forceToolsOnNextMessage INTEGER",
        "allowCrossCharacterVaultReads INTEGER",
        "pendingOutfitNotifications TEXT",
        "state TEXT",
        "compressionCache TEXT",
        "agentModeEnabled INTEGER",
        "agentTurnCount REAL",
        "storyBackgroundImageId TEXT",
        "lastBackgroundGeneratedAt TEXT",
        "imageProfileId TEXT",
        "alertCharactersOfLanternImages INTEGER",
        "isDangerousChat INTEGER",
        "dangerScore REAL",
        "dangerCategories TEXT",
        "dangerClassifiedAt TEXT",
        "dangerClassifiedAtMessageCount REAL",
        "conciergeOverride TEXT",
        "sceneState TEXT",
        "renderedMarkdown TEXT",
        "equippedOutfit TEXT",
        "characterAvatars TEXT",
        "avatarGenerationEnabled INTEGER",
        "chatType TEXT",
        "helpPageUrl TEXT",
        "consoleConnectionProfileId TEXT",
        "compiledIdentityStacks TEXT",
        "courierCheckpoints TEXT",
        "commonplaceSceneCache TEXT",
        "commonplaceRecallHistory TEXT",
        "budgetMaxTurns REAL",
        "budgetMaxTokens REAL",
        "budgetMaxWallClockMs REAL",
        "budgetEstimatedSpendCapUSD REAL",
        "scheduleCron TEXT",
        "scheduleFreshnessWindowMs REAL",
        "scheduleNextRunAt TEXT",
        "scheduleLastRunAt TEXT",
        "runState TEXT",
        "currentRunId TEXT",
        "runStateMessage TEXT",
        "runStartedAt TEXT",
        "runEndedAt TEXT",
        "runPausedAt TEXT",
        "runPausedAccumMs REAL",
        "runTurnsConsumed REAL",
        "runTokensConsumed REAL",
        "runMilestonesAnnounced REAL",
        "runDestructiveToolsAllowed REAL",
        "budgetExcludeCacheHits REAL",
        "runVisibility TEXT",
        "coreWhisperEnabled INTEGER",
        "coreWhisperInterval REAL",
        "showThinking INTEGER",
        "createdAt TEXT",
        "updatedAt TEXT",
        "answerConfirmationOverride TEXT",
        "turnSkippingEnabled INTEGER",
    ];
    format!("CREATE TABLE chats ({});", cols.join(", "))
}

/// Build a fixture instance: `<base>/data/quilltap.db` with the job queue +
/// chats tables (and one seeded chat row for the single user).
fn make_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    w.connection().execute_batch(BACKGROUND_JOBS_DDL).unwrap();
    w.connection().execute_batch(&chats_ddl()).unwrap();
    // The enriched chat-list read (P4.6a) computes `_count` from these tables
    // (the full chat_messages column set the event marshaling SELECTs; empty here).
    w.connection()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_messages (chatId TEXT, id TEXT, type TEXT, role TEXT, \
             content TEXT, rawResponse TEXT, tokenCount TEXT, promptTokens TEXT, completionTokens TEXT, \
             swipeGroupId TEXT, swipeIndex TEXT, attachments TEXT, debugMemoryLogs TEXT, \
             thoughtSignature TEXT, reasoningContent TEXT, reasoningSegments TEXT, participantId TEXT, \
             recoveryType TEXT, renderedHtml TEXT, dangerFlags TEXT, targetParticipantIds TEXT, \
             systemSender TEXT, systemKind TEXT, opaqueContent TEXT, hostEvent TEXT, customAnnouncer TEXT, \
             carinaMeta TEXT, pendingExternalPrompt TEXT, pendingExternalPromptFull TEXT, \
             pendingExternalAttachments TEXT, summaryAnchor TEXT, context TEXT, systemEventType TEXT, \
             description TEXT, totalTokens TEXT, provider TEXT, modelName TEXT, estimatedCostUSD TEXT, \
             createdAt TEXT, isSilentMessage TEXT, confirmed TEXT, confirmationChecked TEXT, \
             confirmationRevised TEXT, confirmationNotes TEXT, confirmationOriginalContent TEXT);\
             CREATE TABLE IF NOT EXISTS memories (id TEXT PRIMARY KEY, chatId TEXT);",
        )
        .unwrap();
    w.connection()
        .execute(
            "INSERT INTO chats (id, userId, title, chatType, messageCount, createdAt, updatedAt) \
             VALUES ('11111111-1111-4111-8111-111111111111', \
                     'ffffffff-ffff-ffff-ffff-ffffffffffff', \
                     'The Reading Room', 'salon', 3, \
                     '2026-01-01T00:00:00.000Z', '2026-01-02T00:00:00.000Z')",
            [],
        )
        .unwrap();
    // The enriched list nests a mount-index read (the vault overlay) — a
    // provisioned Salon instance always has this partition; create an empty one.
    drop(w);
    let _ = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
}

/// A recording handler: counts invocations, completes every job.
struct EchoHandler(Arc<AtomicUsize>);

impl JobHandler for EchoHandler {
    fn handle<'a>(
        &'a self,
        _db: &'a Db,
        _job: &'a quilltap_core::db::background_jobs::BackgroundJob,
    ) -> JobFuture<'a> {
        let count = self.0.clone();
        Box::pin(async move {
            count.fetch_add(1, Ordering::SeqCst);
            JobOutcome::Completed(None)
        })
    }
}

/// Poll the job row until it reaches `want` (or time out).
async fn wait_for_status(db: &Db, job_id: &str, want: &str) {
    for _ in 0..200 {
        let status = queue_service::get_job_status(db, job_id).await.unwrap();
        if status.as_ref().map(|j| j.status.as_str()) == Some(want) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("job {job_id} never reached {want}");
}

fn base_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    // Point the registry somewhere hermetic (the default is the per-user
    // launcher path) and keep the periodic cadences out of the way — the
    // pump's enqueue wake is what these tests exercise.
    config.instances_path = Some(base.join("instances.json"));
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.env_pepper = None; // hermetic: ignore any ambient env pepper
    config
}

/// M0: env-pepper boot (`needs-vault-storage` is operational), dispatches
/// answer, and an enqueued job pumps to COMPLETED through the host's drivers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boots_headless_and_pumps_jobs() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());

    // A registered instance list for the ListInstances dispatch.
    let instances_path = dir.path().join("instances.json");
    std::fs::write(
        &instances_path,
        r#"{"version":1,"instances":{"Fixture":{"path":"/tmp/fixture"}},"defaultInstance":"Fixture"}"#,
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&instances_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    let count = Arc::new(AtomicUsize::new(0));
    let mut config = base_config(dir.path());
    config.env_pepper = Some(PEPPER.to_string());
    let config = config.with_handler("P4_TEST_ECHO", Arc::new(EchoHandler(count.clone())));

    let host = Host::start(config).unwrap();
    let core = host.core();

    // Health: operational via the env pepper (needs-vault-storage).
    match core.dispatch(Request::Health).await {
        Response::Health(h) => {
            assert!(h.ready);
            assert_eq!(h.pepper_state, PepperState::NeedsVaultStorage);
        }
        other => panic!("unexpected: {other:?}"),
    }

    // ListChats: the seeded row, projected.
    match core
        .dispatch(Request::ListChats {
            exclude_tag_ids: vec![],
            limit: None,
            include_autonomous: false,
        })
        .await
    {
        Response::Chats(chats) => {
            assert_eq!(chats.len(), 1);
            assert_eq!(chats[0].title, "The Reading Room");
            assert_eq!(chats[0].chat_type, "salon");
            assert_eq!(chats[0].count.messages, 0);
            assert_eq!(chats[0].last_message_at, None);
        }
        other => panic!("unexpected: {other:?}"),
    }

    // ListInstances: the registry file, pre-unlock surface.
    match core.dispatch(Request::ListInstances).await {
        Response::Instances(dto) => {
            assert_eq!(dto.instances.len(), 1);
            assert_eq!(dto.instances[0].name, "Fixture");
            assert!(dto.instances[0].is_default);
        }
        other => panic!("unexpected: {other:?}"),
    }

    // Enqueue → the wake hook pumps it to COMPLETED through the host loop.
    let db = core.db().expect("engine is ready");
    let job_id = queue_service::enqueue_job(
        &db,
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
        "P4_TEST_ECHO",
        serde_json::json!({}),
        3.0,
    )
    .await
    .unwrap();
    wait_for_status(&db, &job_id, "COMPLETED").await;
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // A second enqueue proves the wake path again (not just the first pump).
    let job2 = queue_service::enqueue_job(
        &db,
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
        "P4_TEST_ECHO",
        serde_json::json!({}),
        3.0,
    )
    .await
    .unwrap();
    wait_for_status(&db, &job2, "COMPLETED").await;
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

/// The lock/unlock cycle against a passphrase-protected `.dbkey` fixture:
/// boots locked, refuses ready-gated dispatch, unlocks (drivers start —
/// proven by pumping a job), locks (engine gone), unlocks again (a fresh
/// assembly pumps again — the wake fan-out survives re-assembly).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn dbkey_lock_unlock_cycle_restarts_drivers() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path());
    dbkey::save_dbkey(&dir.path().join("data"), PEPPER, "open sesame").unwrap();

    let count = Arc::new(AtomicUsize::new(0));
    let config =
        base_config(dir.path()).with_handler("P4_TEST_ECHO", Arc::new(EchoHandler(count.clone())));
    let host = Host::start(config).unwrap();
    let core = host.core();

    // Locked boot.
    match core.dispatch(Request::Health).await {
        Response::Health(h) => {
            assert!(!h.ready);
            assert_eq!(h.pepper_state, PepperState::NeedsPassphrase);
        }
        other => panic!("unexpected: {other:?}"),
    }
    match core
        .dispatch(Request::ListChats {
            exclude_tag_ids: vec![],
            limit: None,
            include_autonomous: false,
        })
        .await
    {
        Response::Error(e) => {
            assert_eq!(e.kind, ErrorKind::Locked);
            assert_eq!(e.pepper_state, Some(PepperState::NeedsPassphrase));
        }
        other => panic!("unexpected: {other:?}"),
    }

    // Unlock; the drivers come up and pump.
    match core
        .dispatch(Request::Unlock {
            passphrase: "open sesame".into(),
        })
        .await
    {
        Response::UnlockState(u) => {
            assert_eq!(u.state, PepperState::Resolved);
            assert!(u.has_user_passphrase);
        }
        other => panic!("unexpected: {other:?}"),
    }
    let db = core.db().unwrap();
    let job = queue_service::enqueue_job(
        &db,
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
        "P4_TEST_ECHO",
        serde_json::json!({}),
        3.0,
    )
    .await
    .unwrap();
    wait_for_status(&db, &job, "COMPLETED").await;
    assert_eq!(count.load(Ordering::SeqCst), 1);
    drop(db);

    // Lock: engine gone, gated again.
    match core.dispatch(Request::Lock).await {
        Response::UnlockState(u) => assert_eq!(u.state, PepperState::NeedsPassphrase),
        other => panic!("unexpected: {other:?}"),
    }
    assert!(core.db().is_none());

    // Unlock again: a FRESH assembly (new runner, new loops) pumps.
    match core
        .dispatch(Request::Unlock {
            passphrase: "open sesame".into(),
        })
        .await
    {
        Response::UnlockState(u) => assert_eq!(u.state, PepperState::Resolved),
        other => panic!("unexpected: {other:?}"),
    }
    let db = core.db().unwrap();
    let job = queue_service::enqueue_job(
        &db,
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
        "P4_TEST_ECHO",
        serde_json::json!({}),
        3.0,
    )
    .await
    .unwrap();
    wait_for_status(&db, &job, "COMPLETED").await;
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
