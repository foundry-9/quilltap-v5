//! P4.1d integration tests: the instance lock in the boot path (conflict →
//! typed boot error; loss → drivers stop + handler) and the scheduler sweep
//! loops (the maintenance startup tick honoring the 20 h window across a
//! re-boot; the danger-scan loop's all-OFF start gate + a real enqueue).
//!
//! Like `host_boot.rs`, the fixture is a hand-rolled instance directory with
//! loose-typed tables — the tier-2 differentials own real-schema fidelity;
//! these tests own the host composition (loops, gates, lock).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use quilltap_core::api::{QuilltapCore as _, Request, Response};
use quilltap_core::db::runtime::Db;
use quilltap_core::db::Writer;
use quilltap_host::{lock, Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

const BACKGROUND_JOBS_DDL: &str = "CREATE TABLE background_jobs (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, type TEXT NOT NULL, status TEXT NOT NULL, \
    payload TEXT NOT NULL, priority REAL NOT NULL, attempts REAL NOT NULL, \
    maxAttempts REAL NOT NULL, lastError TEXT, scheduledAt TEXT NOT NULL, \
    startedAt TEXT, completedAt TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

const INSTANCE_SETTINGS_DDL: &str = "CREATE TABLE instance_settings (\
    \"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);";

const TERMINAL_SESSIONS_DDL: &str = "CREATE TABLE terminal_sessions (\
    id TEXT PRIMARY KEY, chatId TEXT NOT NULL, label TEXT, shell TEXT NOT NULL, \
    cwd TEXT NOT NULL, startedAt TEXT NOT NULL, exitedAt TEXT, exitCode REAL, \
    transcriptPath TEXT, createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

const CHAT_SETTINGS_DDL: &str = "CREATE TABLE chat_settings (\
    id TEXT PRIMARY KEY, userId TEXT NOT NULL, autoHousekeepingSettings TEXT, \
    llmLoggingSettings TEXT, dangerousContentSettings TEXT, \
    createdAt TEXT NOT NULL, updatedAt TEXT NOT NULL);";

const CONNECTION_PROFILES_DDL: &str = "CREATE TABLE connection_profiles (\
    id TEXT PRIMARY KEY, userId TEXT, name TEXT, provider TEXT, transport TEXT, \
    courierDeltaMode INTEGER, apiKeyId TEXT, baseUrl TEXT, modelName TEXT, parameters TEXT, \
    isDefault INTEGER, isCheap INTEGER, allowWebSearch INTEGER, useNativeWebSearch INTEGER, \
    allowToolUse INTEGER, pseudoToolMode TEXT, modelClass TEXT, maxContext REAL, maxTokens REAL, \
    isDangerousCompatible INTEGER, supportsImageUpload INTEGER, tags TEXT, sortIndex REAL, \
    totalTokens REAL, totalPromptTokens REAL, totalCompletionTokens REAL, messageCount REAL, \
    createdAt TEXT, updatedAt TEXT);";

/// The full loose `chats` column set (host_boot's shape).
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
    ];
    format!("CREATE TABLE chats ({});", cols.join(", "))
}

fn make_instance(base: &Path, danger_mode: &str) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    let c = w.connection();
    c.execute_batch(BACKGROUND_JOBS_DDL).unwrap();
    c.execute_batch(INSTANCE_SETTINGS_DDL).unwrap();
    c.execute_batch(TERMINAL_SESSIONS_DDL).unwrap();
    c.execute_batch(CHAT_SETTINGS_DDL).unwrap();
    c.execute_batch(CONNECTION_PROFILES_DDL).unwrap();
    c.execute_batch(&chats_ddl()).unwrap();

    // One chat_settings row; the danger sub-object drives the scan gate.
    let dcs = format!(
        r#"{{"mode":"{danger_mode}","threshold":0.7,"scanTextChat":true,"scanImagePrompts":false,"scanImageGeneration":false,"displayMode":"SHOW","showWarningBadges":true}}"#
    );
    c.execute(
        "INSERT INTO chat_settings (id, userId, dangerousContentSettings, createdAt, updatedAt) \
         VALUES ('a0000000-0000-4000-8000-00000000000a', ?1, ?2, \
                 '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        rusqlite::params![USER, dcs],
    )
    .unwrap();

    // One never-classified salon chat + one connection profile (the fallback).
    c.execute(
        "INSERT INTO chats (id, userId, title, chatType, messageCount, createdAt, updatedAt) \
         VALUES ('11111111-1111-4111-8111-111111111111', ?1, 'Scanned', 'salon', 3, \
                 '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        rusqlite::params![USER],
    )
    .unwrap();
    c.execute(
        "INSERT INTO connection_profiles \
         (id, userId, name, provider, transport, courierDeltaMode, modelName, parameters, \
          isDefault, isCheap, allowWebSearch, useNativeWebSearch, allowToolUse, pseudoToolMode, \
          isDangerousCompatible, supportsImageUpload, tags, sortIndex, totalTokens, \
          totalPromptTokens, totalCompletionTokens, messageCount, createdAt, updatedAt) \
         VALUES ('p0000000-0000-4000-8000-00000000000b', ?1, 'Main', 'ANTHROPIC', 'api', 0, \
                 'claude-sonnet-4-5', '{}', 1, 0, 0, 0, 1, 'AUTO', 0, 0, '[]', 0, 0, 0, 0, 0, \
                 '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        rusqlite::params![USER],
    )
    .unwrap();
}

/// A base config with every cadence pushed out of the way; tests pull in the
/// loop under test.
fn quiet_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    let hour = 3_600_000;
    config.autonomous_tick_ms = hour;
    config.stuck_check_ms = hour;
    config.cleanup_interval_ms = hour;
    config.housekeeping_interval_ms = hour;
    config.maintenance_interval_ms = hour;
    config.danger_scan_interval_ms = hour;
    config.startup_grace_ms = hour;
    config.heartbeat_ms = hour;
    config
}

async fn wait_until(mut probe: impl FnMut() -> bool, what: &str) {
    for _ in 0..400 {
        if probe() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn count_jobs(db: &Db, job_type: &str) -> i64 {
    let t = job_type.to_string();
    db.read_main(move |c| {
        Ok(c.query_row(
            "SELECT COUNT(*) FROM background_jobs WHERE type = ?1",
            rusqlite::params![t],
            |r| r.get::<_, i64>(0),
        )?)
    })
    .unwrap()
}

/// A live conflicting lock (PID 1 on this host) refuses the boot with the
/// typed error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_lock_conflict_is_a_boot_error() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path(), "OFF");

    let lock_path = lock::instance_lock_path(dir.path());
    std::fs::write(
        &lock_path,
        format!(
            r#"{{"pid": 1, "hostname": "{}", "startedAt": "2026-01-01T00:00:00.000Z",
"lastHeartbeat": "2026-01-01T00:00:00.000Z", "environment": "local",
"processTitle": "node", "processArgv0": "/usr/bin/node", "history": []}}"#,
            lock::hostname()
        ),
    )
    .unwrap();

    let err = match Host::start(quiet_config(dir.path())) {
        Err(e) => e,
        Ok(_) => panic!("boot must refuse"),
    };
    let msg = err.to_string();
    assert!(
        msg.contains("Another Quilltap instance") && msg.contains("PID 1"),
        "unexpected error: {msg}"
    );
    // The holder's file is untouched.
    assert_eq!(lock::read_lock_file(&lock_path).unwrap().pid, 1);
}

/// The boot acquires the lock; Lock (teardown) releases it; a lost lock stops
/// the drivers and fires the injected handler instead of exiting.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lock_lifecycle_and_loss_handler() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path(), "OFF");

    let lost = Arc::new(AtomicUsize::new(0));
    let mut config = quiet_config(dir.path());
    config.heartbeat_ms = 50;
    let lost_clone = lost.clone();
    config.on_lock_lost = Some(Arc::new(move || {
        lost_clone.fetch_add(1, Ordering::SeqCst);
    }));

    let host = Host::start(config).unwrap();
    let core = host.core().clone();
    let lock_path = lock::instance_lock_path(dir.path());

    // Boot acquired the lock as this process.
    let content = lock::read_lock_file(&lock_path).expect("lock held");
    assert_eq!(content.pid, std::process::id());

    // The heartbeat rewrites lastHeartbeat.
    let before = content.last_heartbeat.clone();
    wait_until(
        || {
            lock::read_lock_file(&lock_path)
                .map(|c| c.last_heartbeat > before)
                .unwrap_or(false)
        },
        "heartbeat rewrite",
    )
    .await;

    // Simulate a takeover: the file vanishes → the handler fires (no exit).
    std::fs::remove_file(&lock_path).unwrap();
    wait_until(|| lost.load(Ordering::SeqCst) > 0, "lock-loss handler").await;

    // A fresh boot cycle: Lock dispatch releases cleanly (no file left) and
    // Unlock is impossible here (env pepper) — so just verify release via a
    // second host on a fresh dir.
    drop(core);

    let dir2 = tempfile::tempdir().unwrap();
    make_instance(dir2.path(), "OFF");
    let host2 = Host::start(quiet_config(dir2.path())).unwrap();
    let lock2 = lock::instance_lock_path(dir2.path());
    assert!(lock2.exists());
    match host2.core().dispatch(Request::Lock).await {
        Response::UnlockState(_) => {}
        other => panic!("unexpected: {other:?}"),
    }
    assert!(
        !lock2.exists(),
        "Lock dispatch must release the instance lock"
    );
}

/// The maintenance loop: the startup tick runs after the grace, records
/// `lastMaintenanceSweepAt`, and a re-boot within the 20 h window SKIPS the
/// startup tick; an aged stamp runs it again.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maintenance_startup_tick_honors_recent_run_window() {
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path(), "OFF");

    // An old closed terminal session — the sweep's observable effect.
    async fn seed_session(db: &Db, id: &str) {
        let id = id.to_string();
        db.write(move |w| {
            w.main().connection().execute(
                "INSERT INTO terminal_sessions \
                 (id, chatId, shell, cwd, startedAt, exitedAt, createdAt, updatedAt) \
                 VALUES (?1, 'c1', 'zsh', '/tmp', '2020-01-01T00:00:00.000Z', \
                         '2020-06-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z', \
                         '2020-01-01T00:00:00.000Z')",
                rusqlite::params![id],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    }
    let session_count = |db: &Db| -> i64 {
        db.read_main(|c| {
            Ok(
                c.query_row("SELECT COUNT(*) FROM terminal_sessions", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        })
        .unwrap()
    };

    let mut config = quiet_config(dir.path());
    config.startup_grace_ms = 100;
    let host = Host::start(config).unwrap();
    let core = host.core().clone();
    let db = core.db().unwrap();
    seed_session(&db, "s-1").await;

    // The startup tick (after the 100 ms grace) reaps it + records the stamp.
    wait_until(|| session_count(&db) == 0, "first maintenance startup tick").await;
    let stamp = db
        .read_main(quilltap_core::db::instance_settings::get_last_maintenance_sweep_at)
        .unwrap();
    assert!(stamp.is_some(), "stamp recorded");
    drop(db);

    // Re-boot (Lock → Unlock is unavailable under an env pepper; use a full
    // teardown + fresh Host). The fresh stamp is within 20 h → startup SKIPPED.
    match core.dispatch(Request::Lock).await {
        Response::UnlockState(_) => {}
        other => panic!("unexpected: {other:?}"),
    }
    let mut config = quiet_config(dir.path());
    config.startup_grace_ms = 100;
    let host2 = Host::start(config).unwrap();
    let db = host2.core().db().unwrap();
    seed_session(&db, "s-2").await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        session_count(&db),
        1,
        "recent stamp must skip the startup maintenance tick"
    );

    // Age the stamp past 20 h → a re-boot runs the startup tick again.
    match host2.core().dispatch(Request::Lock).await {
        Response::UnlockState(_) => {}
        other => panic!("unexpected: {other:?}"),
    }
    let now_ms = quilltap_core::clock::now_unix_ms();
    let old_iso = quilltap_core::clock::iso_from_unix_ms(now_ms - 21 * 60 * 60 * 1000);
    {
        let w =
            Writer::open_writable(&dir.path().join("data").join("quilltap.db"), PEPPER).unwrap();
        w.connection()
            .execute(
                "UPDATE instance_settings SET value = ?1 WHERE \"key\" = 'lastMaintenanceSweepAt'",
                rusqlite::params![old_iso],
            )
            .unwrap();
    }
    let mut config = quiet_config(dir.path());
    config.startup_grace_ms = 100;
    let host3 = Host::start(config).unwrap();
    let db = host3.core().db().unwrap();
    wait_until(
        || session_count(&db) == 0,
        "aged stamp re-runs the startup tick",
    )
    .await;
}

/// The danger-scan loop: mode-on scans immediately (a classification job rows
/// in at priority -2); all-OFF never starts the loop.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn danger_scan_gate_and_enqueue() {
    // Mode ON → the immediate scan enqueues for the never-classified chat.
    let dir = tempfile::tempdir().unwrap();
    make_instance(dir.path(), "DETECT_ONLY");
    let mut config = quiet_config(dir.path());
    config.danger_scan_interval_ms = 3_600_000;
    let host = Host::start(config).unwrap();
    let db = host.core().db().unwrap();
    // Direct-drive first so a sweep error surfaces loudly (the loop swallows).
    let (users, enq) = quilltap_core::services::danger_scan::run_scheduled_danger_scan(&db)
        .await
        .expect("danger scan runs clean");
    assert_eq!((users, enq), (1, 1), "direct scan counts");
    // The loop's own immediate scan runs concurrently with the direct drive
    // above; the enqueue dedupe is best-effort (v4's find-then-insert has the
    // same window between two concurrent scans), so 1 or 2 rows are both
    // correct — what matters is that the loop enqueued at all, with the right
    // payload + priority.
    wait_until(
        || count_jobs(&db, "CHAT_DANGER_CLASSIFICATION") >= 1,
        "danger-scan enqueue",
    )
    .await;
    let (payload, priority): (String, f64) = db
        .read_main(|c| {
            Ok(c.query_row(
                "SELECT payload, priority FROM background_jobs WHERE type = 'CHAT_DANGER_CLASSIFICATION' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?)
        })
        .unwrap();
    assert_eq!(priority, -2.0);
    assert!(payload.contains("11111111-1111-4111-8111-111111111111"));
    assert!(payload.contains("p0000000-0000-4000-8000-00000000000b"));

    // All-OFF → the loop never starts; no job appears.
    let dir2 = tempfile::tempdir().unwrap();
    make_instance(dir2.path(), "OFF");
    let mut config = quiet_config(dir2.path());
    config.danger_scan_interval_ms = 50;
    let host2 = Host::start(config).unwrap();
    let db2 = host2.core().db().unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(count_jobs(&db2, "CHAT_DANGER_CLASSIFICATION"), 0);
}
