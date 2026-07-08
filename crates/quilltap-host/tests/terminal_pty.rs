//! Integration tests for the PTY host driver (P4.1c) — a REAL PTY per test.
//!
//! Each test spawns `/bin/sh` into a fresh portable-pty session over a
//! temp-dir instance (an encrypted main DB with a hand-rolled
//! `terminal_sessions` table — the announcement writers no-op gracefully on
//! the missing `chats` table, v4's catch-all shape) and drives the manager
//! end to end: output → subscribers/ring buffer/transcript, the exit
//! sequence stamping the DB row, the attach replay, kill/write/resize, the
//! ring-buffer cap, the Ariel flush drivers, and the production scrollback
//! source.
//!
//! If PTY spawning is unavailable (a locked-down CI sandbox), each test
//! SKIPs loudly rather than failing — but they run for real on a dev
//! machine (the work order's "must run and pass locally").

#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::{terminal_sessions, Writer};
use quilltap_core::jsstr::utf16_len;
use quilltap_core::tools::executor::TerminalScrollbackSource;
use quilltap_host::terminal::protocol::WsServerMessage;
use quilltap_host::terminal::scrollback::PtyScrollbackSource;
use quilltap_host::terminal::{SpawnOptions, TerminalManager, TerminalManagerConfig};

/// The differential test pepper (32 bytes, base64) — same as the core tests.
const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

const CHAT_ID: &str = "c0000001-0000-4000-8000-0000000000c1";

struct TestInstance {
    _dir: tempfile::TempDir,
    db: Db,
    files_dir: std::path::PathBuf,
    logs_dir: std::path::PathBuf,
    data_dir: std::path::PathBuf,
}

/// Build a temp instance: an encrypted main DB carrying only the
/// `terminal_sessions` table (v4's generated DDL shape — `exitCode` REAL per
/// `mapToSQLiteType`'s no-max integer rule).
fn test_instance() -> TestInstance {
    let dir = tempfile::tempdir().expect("tempdir");
    let main_path = dir.path().join("main.db");
    {
        let w = Writer::open_writable(&main_path, PEPPER).expect("open writable");
        w.connection()
            .execute_batch(
                "CREATE TABLE terminal_sessions (\
                   id TEXT PRIMARY KEY, chatId TEXT, label TEXT, shell TEXT, \
                   cwd TEXT, startedAt TEXT, exitedAt TEXT, exitCode REAL, \
                   transcriptPath TEXT, createdAt TEXT, updatedAt TEXT);",
            )
            .expect("create terminal_sessions");
    }
    let db = Db::open(
        DbPaths {
            main: main_path,
            mount_index: None,
            llm_logs: None,
        },
        PEPPER,
    )
    .expect("open db");

    let files_dir = dir.path().join("files");
    let logs_dir = dir.path().join("logs");
    std::fs::create_dir_all(&files_dir).unwrap();
    std::fs::create_dir_all(&logs_dir).unwrap();
    let data_dir = dir.path().to_path_buf();

    TestInstance {
        _dir: dir,
        db,
        files_dir,
        logs_dir,
        data_dir,
    }
}

fn manager_with(inst: &TestInstance, idle: Duration, max_age: Duration) -> Arc<TerminalManager> {
    let mut cfg = TerminalManagerConfig::new(
        inst.db.clone(),
        inst.files_dir.clone(),
        inst.logs_dir.clone(),
        inst.data_dir.clone(),
    );
    cfg.ariel_flush_idle = idle;
    cfg.ariel_flush_max_age = max_age;
    TerminalManager::new(cfg)
}

/// Spawn `/bin/sh` or SKIP the test when the environment forbids PTYs.
macro_rules! spawn_or_skip {
    ($mgr:expr, $opts:expr) => {
        match $mgr.spawn($opts).await {
            Ok(meta) => meta,
            Err(e) => {
                eprintln!("SKIP: PTY spawn unavailable in this environment: {e}");
                return;
            }
        }
    };
}

fn sh_opts() -> SpawnOptions {
    SpawnOptions {
        chat_id: CHAT_ID.to_string(),
        shell: Some("/bin/sh".to_string()),
        ..Default::default()
    }
}

fn read_row(db: &Db, id: &str) -> Option<terminal_sessions::TerminalSessionRow> {
    let id = id.to_string();
    db.read_main(move |conn| terminal_sessions::find_by_id(conn, &id))
        .expect("read row")
}

/// Poll until `pred` is true or the timeout elapses.
async fn wait_until<F: FnMut() -> bool>(mut pred: F, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if pred() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    pred()
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_output_exit_and_db_row() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());

    // The DB row landed at spawn, pinned to the session id, live.
    let row = read_row(&inst.db, &meta.id).expect("row at spawn");
    assert_eq!(row.chat_id, CHAT_ID);
    assert_eq!(row.shell, "/bin/sh");
    assert_eq!(row.started_at, meta.started_at);
    assert!(row.exited_at.is_none());
    assert!(row.transcript_path.is_some());

    // Subscribe (attach replay first: one output frame + one meta frame).
    let mut sub = mgr.subscribe(&meta.id).expect("subscribe");
    let first = sub.rx.recv().await.expect("replay output");
    assert!(matches!(first, WsServerMessage::Output { .. }));
    let second = sub.rx.recv().await.expect("replay meta");
    match second {
        WsServerMessage::Meta { meta: m } => assert_eq!(m.id, meta.id),
        other => panic!("expected meta frame, got {other:?}"),
    }

    // Drive the shell: emit a marker, then exit 0.
    assert!(mgr.write(&meta.id, "printf 'marker123\\n'; exit 0\n"));

    // Collect frames until the exit frame arrives.
    let mut saw_marker = false;
    let exit_code;
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(15), sub.rx.recv())
            .await
            .expect("frame within 15s");
        match msg {
            Some(WsServerMessage::Output { data }) => {
                if data.contains("marker123") {
                    saw_marker = true;
                }
            }
            Some(WsServerMessage::Exit { code, .. }) => {
                exit_code = code;
                break;
            }
            Some(_) => {}
            None => panic!("channel closed before exit frame"),
        }
    }
    assert!(saw_marker, "subscriber saw the marker output");
    assert_eq!(exit_code, Some(0));

    // The exit sequence stamps the DB row (after the exit broadcast).
    assert!(
        wait_until(
            || read_row(&inst.db, &meta.id)
                .map(|r| r.exited_at.is_some())
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await,
        "exitedAt stamped"
    );
    let row = read_row(&inst.db, &meta.id).unwrap();
    assert_eq!(row.exit_code, Some(0));

    // Ring buffer + transcript both carry the marker.
    let ring = mgr.get_ring_buffer(&meta.id).expect("ring buffer");
    assert!(ring.contains("marker123"));
    let transcript = std::fs::read_to_string(row.transcript_path.unwrap()).unwrap();
    assert!(transcript.contains("marker123"));

    // The channel closes after exit (v4's close(1000)); unsubscribing the
    // exited session removes it from the map.
    assert!(sub.rx.recv().await.is_none());
    assert!(mgr.contains(&meta.id));
    mgr.unsubscribe(&meta.id, sub.id);
    assert!(!mgr.contains(&meta.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_after_output_replays_ring_buffer() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());

    assert!(mgr.write(&meta.id, "printf 'hello_replay\\n'\n"));
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2
                .get_ring_buffer(&id)
                .map(|r| r.contains("hello_replay"))
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await,
        "ring buffer filled before attach"
    );

    // A late subscriber gets the whole ring buffer as ONE output frame, then meta.
    let mut sub = mgr.subscribe(&meta.id).expect("subscribe");
    match sub.rx.recv().await.expect("replay output") {
        WsServerMessage::Output { data } => {
            assert_eq!(data, mgr.get_ring_buffer(&meta.id).unwrap());
            assert!(data.contains("hello_replay"));
        }
        other => panic!("expected output replay, got {other:?}"),
    }
    match sub.rx.recv().await.expect("replay meta") {
        WsServerMessage::Meta { meta: m } => {
            assert_eq!(m.chat_id, CHAT_ID);
            assert!(m.exited_at.is_none());
        }
        other => panic!("expected meta frame, got {other:?}"),
    }

    // End the session (an interactive shell ignores SIGTERM; exit is enough).
    let _ = mgr.write(&meta.id, "exit\n");
    // Reader thread finishes; nothing else to assert here.
    let mgr3 = Arc::clone(&mgr);
    let id = meta.id.clone();
    wait_until(
        move || {
            mgr3.session_meta(&id)
                .map(|m| m.exited_at.is_some())
                .unwrap_or(true)
        },
        Duration::from_secs(10),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn ring_buffer_caps_to_the_last_256k_units() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(120), Duration::from_secs(240));
    let meta = spawn_or_skip!(mgr, sh_opts());

    // A head marker, then ~300 KB of 'x', then a tail marker — the head must
    // be evicted. Markers are built by CONCATENATED printf arguments so the
    // tty ECHO of the typed command never contains the assembled marker (the
    // echo bit an earlier version of this test: `wait_until` matched the
    // echoed command text, not the loop output).
    let cmd = "printf '%s%s\\n' HEAD MARK; i=0; while [ $i -lt 300 ]; do \
               head -c 1024 /dev/zero | tr '\\0' 'x'; echo; i=$((i+1)); done; \
               printf '%s%s\\n' TAIL MARK\n";
    assert!(mgr.write(&meta.id, cmd));

    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2
                .get_ring_buffer(&id)
                .map(|r| r.contains("TAILMARK"))
                .unwrap_or(false),
            Duration::from_secs(60),
        )
        .await,
        "big output completed"
    );

    let ring = mgr.get_ring_buffer(&meta.id).unwrap();
    assert!(utf16_len(&ring) <= 256 * 1024, "ring capped at 256K units");
    assert!(ring.contains("TAILMARK"), "tail kept");
    assert!(!ring.contains("HEADMARK"), "head evicted");

    let _ = mgr.write(&meta.id, "exit\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn kill_terminates_and_stamps_the_row() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());

    // An interactive shell IGNORES SIGTERM (bash/sh job-control behavior — the
    // same is true under v4's node-pty `kill('SIGTERM')`), so replace the shell
    // with a process that dies on it.
    assert!(mgr.write(&meta.id, "exec sleep 1000\n"));
    tokio::time::sleep(Duration::from_millis(300)).await;

    mgr.kill(&meta.id);

    assert!(
        wait_until(
            || read_row(&inst.db, &meta.id)
                .map(|r| r.exited_at.is_some())
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await,
        "kill stamps exitedAt"
    );
    // Signaled child: the in-memory meta agrees with the DB row.
    let m = mgr.session_meta(&meta.id).expect("still in map");
    assert!(m.exited_at.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn write_resize_and_subscribe_guards() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());

    // Unknown-session paths.
    assert!(!mgr.write("nope", "x"));
    mgr.resize("nope", 80, 24); // swallow
    assert!(mgr.subscribe("nope").is_none());
    assert!(mgr.get_ring_buffer("nope").is_none());

    // Live paths.
    mgr.resize(&meta.id, 100, 30);
    assert!(mgr.write(&meta.id, "exit 3\n"));

    assert!(
        wait_until(
            || read_row(&inst.db, &meta.id)
                .map(|r| r.exited_at.is_some())
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await
    );
    let row = read_row(&inst.db, &meta.id).unwrap();
    assert_eq!(row.exit_code, Some(3));

    // Subscribe refuses an exited session (v4's exitedAt !== null check).
    assert!(mgr.subscribe(&meta.id).is_none());
    // Write to an exited session fails (writer dropped).
    assert!(!mgr.write(&meta.id, "echo nope\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ariel_idle_flush_drains_the_buffer() {
    let inst = test_instance();
    // Short idle window; long max-age so idle is the firing deadline.
    let mgr = manager_with(&inst, Duration::from_millis(200), Duration::from_secs(60));
    let meta = spawn_or_skip!(mgr, sh_opts());

    assert!(mgr.write(&meta.id, "printf 'ariel_payload\\n'\n"));
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2.ariel_buffer_len(&id).map(|n| n > 0).unwrap_or(false),
            Duration::from_secs(10),
        )
        .await,
        "output buffered for Ariel"
    );

    // After the idle window (with no further output) the driver flushes: the
    // buffer drains even though the announcement no-ops here (no chats table —
    // the writer's error-swallow shape).
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2.ariel_buffer_len(&id) == Some(0),
            Duration::from_secs(10),
        )
        .await,
        "idle flush drained the Ariel buffer"
    );

    let _ = mgr.write(&meta.id, "exit\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn ariel_max_age_flush_fires_under_steady_drip() {
    let inst = test_instance();
    // Idle window effectively unreachable next to the drip; max-age short.
    let mgr = manager_with(&inst, Duration::from_secs(60), Duration::from_millis(400));
    let meta = spawn_or_skip!(mgr, sh_opts());

    // A steady drip: output every 100 ms, forever (killed at the end).
    assert!(mgr.write(&meta.id, "while true; do printf drip; sleep 0.1; done\n"));

    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2.ariel_buffer_len(&id).map(|n| n > 0).unwrap_or(false),
            Duration::from_secs(10),
        )
        .await,
        "drip buffered"
    );

    // The idle deadline keeps moving; the max-age cap still fires and drains.
    // Observe a drain by watching the buffer length drop below its running max.
    let mut drained = false;
    let mut seen_max = 0usize;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        if let Some(n) = mgr.ariel_buffer_len(&meta.id) {
            if n < seen_max {
                drained = true;
                break;
            }
            seen_max = seen_max.max(n);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(drained, "max-age flush drained the buffer mid-drip");

    // Break the drip loop and end the shell (SIGINT-free path: close via exit
    // after ^C; simplest is SIGKILL through the portable-pty killer — but the
    // loop ignores nothing, so ^C then exit does it deterministically).
    let _ = mgr.write(&meta.id, "\u{3}exit\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn kick_all_for_chat_removes_and_kills() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta1 = spawn_or_skip!(mgr, sh_opts());
    let meta2 = spawn_or_skip!(
        mgr,
        SpawnOptions {
            chat_id: "other-chat".to_string(),
            shell: Some("/bin/sh".to_string()),
            ..Default::default()
        }
    );

    // SIGTERM-killable stand-in for the interactive shell (see the kill test).
    assert!(mgr.write(&meta1.id, "exec sleep 1000\n"));
    tokio::time::sleep(Duration::from_millis(300)).await;

    mgr.kick_all_for_chat(CHAT_ID).await;
    assert!(!mgr.contains(&meta1.id), "kicked session left the map");
    assert!(mgr.contains(&meta2.id), "other chat untouched");

    // The reaped child still stamps its row via the exit sequence.
    assert!(
        wait_until(
            || read_row(&inst.db, &meta1.id)
                .map(|r| r.exited_at.is_some())
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await,
        "kicked session's exit sequence still stamped the row"
    );

    let _ = mgr.write(&meta2.id, "exit\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn scrollback_source_live_then_transcript() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());
    let source = PtyScrollbackSource::new(Arc::clone(&mgr), inst.db.clone());

    assert!(mgr.write(&meta.id, "printf 'scroll_marker\\n'\n"));
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2
                .get_ring_buffer(&id)
                .map(|r| r.contains("scroll_marker"))
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await
    );

    // Live session + exitedAt null → the ring buffer verbatim.
    let live = source.scrollback(&meta.id);
    assert_eq!(live, mgr.get_ring_buffer(&meta.id).unwrap());
    assert!(live.contains("scroll_marker"));

    // After exit the row's exitedAt is set → the transcript tail (even though
    // the session lingers in the map until unsubscribed).
    assert!(mgr.write(&meta.id, "exit 0\n"));
    assert!(
        wait_until(
            || read_row(&inst.db, &meta.id)
                .map(|r| r.exited_at.is_some())
                .unwrap_or(false),
            Duration::from_secs(10),
        )
        .await
    );
    assert!(mgr.contains(&meta.id), "exited session lingers in the map");
    let post_exit = source.scrollback(&meta.id);
    assert!(
        post_exit.contains("scroll_marker"),
        "transcript tail served"
    );

    // Unknown session → empty (the seam's total contract).
    assert_eq!(source.scrollback("does-not-exist"), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn reconcile_marks_orphans_and_spares_live_sessions() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));

    // A live session for this chat.
    let live = spawn_or_skip!(mgr, sh_opts());

    // An orphan row: exitedAt null, no live PTY (a pre-restart leftover).
    let orphan_id = "0e000000-0000-4000-8000-00000000000e".to_string();
    {
        let orphan_id = orphan_id.clone();
        inst.db
            .write(move |writers| {
                writers.main().terminal_sessions().create(
                    &terminal_sessions::TsCreate {
                        chat_id: CHAT_ID.to_string(),
                        label: None,
                        shell: "/bin/sh".to_string(),
                        cwd: "/tmp".to_string(),
                        started_at: "2026-01-01T00:00:00.000Z".to_string(),
                        exited_at: None,
                        exit_code: None,
                        transcript_path: None,
                    },
                    &terminal_sessions::CreateOptions {
                        id: orphan_id,
                        created_at: "2026-01-01T00:00:00.000Z".to_string(),
                        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
                    },
                )
            })
            .await
            .expect("seed orphan");
    }

    let reconciled = mgr.reconcile_for_chat(CHAT_ID).await;
    assert_eq!(reconciled, 1, "one orphan reconciled");

    let orphan = read_row(&inst.db, &orphan_id).expect("orphan row");
    assert!(orphan.exited_at.is_some());
    assert_eq!(orphan.exit_code, None, "explicit NULL exit code");

    // The live session was spared (is_live probe).
    let live_row = read_row(&inst.db, &live.id).expect("live row");
    assert!(live_row.exited_at.is_none());

    // Idempotent: a second pass reconciles nothing.
    assert_eq!(mgr.reconcile_for_chat(CHAT_ID).await, 0);

    let _ = mgr.write(&live.id, "exit\n");
}
