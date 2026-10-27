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
use quilltap_core::terminal_screen::ScreenBufferKind;
use quilltap_core::tools::executor::TerminalReadSource;
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
                   transcriptPath TEXT, createdAt TEXT, updatedAt TEXT, \
                   cols REAL, rows REAL);",
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
            move || mgr2.ariel_pending(&id) == Some(true),
            Duration::from_secs(10),
        )
        .await,
        "output pending an Ariel summary"
    );

    // After the idle window (with no further output) the driver flushes: the
    // pending mark clears even though the announcement no-ops here (no chats
    // table — the writer's error-swallow shape).
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2.ariel_pending(&id) == Some(false),
            Duration::from_secs(10),
        )
        .await,
        "idle flush took the pending summary"
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
            move || mgr2.ariel_pending(&id) == Some(true),
            Duration::from_secs(10),
        )
        .await,
        "drip pending"
    );

    // The idle deadline keeps moving; the max-age cap still fires. Observe the
    // flush as the pending mark clearing between two drips (100 ms apart).
    let mut flushed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline {
        if mgr.ariel_pending(&meta.id) == Some(false) {
            flushed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(flushed, "max-age flush took the summary mid-drip");

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

    // Live session + exitedAt null → the live screen rendered, and the ring
    // buffer verbatim as the raw stream.
    let live = source.read(&meta.id);
    assert_eq!(live.raw, mgr.get_ring_buffer(&meta.id).unwrap());
    assert!(live.raw.contains("scroll_marker"));
    let rendering = live.rendering.expect("a live rendering");
    assert_eq!(rendering.buffer, ScreenBufferKind::Normal);
    assert!(
        rendering.lines.iter().any(|l| l == "scroll_marker"),
        "the echoed line, rendered: {:?}",
        rendering.lines
    );
    assert_eq!((rendering.cols, rendering.rows), (80, 24));

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
    let post_exit = source.read(&meta.id);
    assert!(
        post_exit.raw.contains("scroll_marker"),
        "transcript tail served"
    );
    let replay = post_exit.rendering.expect("the transcript replayed");
    assert!(replay.lines.iter().any(|l| l == "scroll_marker"));
    // Replayed at the stored geometry (spawn-time 80×24, stamped again on exit).
    let row = read_row(&inst.db, &meta.id).unwrap();
    assert_eq!((row.cols, row.rows), (Some(80), Some(24)));
    assert_eq!((replay.cols, replay.rows), (80, 24));

    // Unknown session → nothing (the seam's total contract).
    let unknown = source.read("does-not-exist");
    assert!(unknown.rendering.is_none() && unknown.raw.is_empty());
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
                        cols: None,
                        rows: None,
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

/// The manager as the engine's `TerminalLivenessProbe` (the `ChatGet` reconcile
/// seam): live → true, unknown → false, and an exited-but-still-subscribed
/// session lingers as true — the same truthiness v4's `ptyManager.get` gives
/// the reconcile pass (harmless there: the DB row is already exit-stamped).
#[tokio::test(flavor = "multi_thread")]
async fn manager_is_the_chat_get_liveness_probe() {
    use quilltap_core::services::ariel_notifications::TerminalLivenessProbe;

    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let probe: Arc<dyn TerminalLivenessProbe> = mgr.clone();

    let meta = spawn_or_skip!(mgr, sh_opts());
    assert!(probe.is_live(&meta.id), "spawned session is live");
    assert!(!probe.is_live("does-not-exist"), "unknown id is not live");

    // Hold a subscription across the exit so the session lingers in the map.
    let _sub = mgr.subscribe(&meta.id).expect("subscribe");
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
    assert!(
        probe.is_live(&meta.id),
        "exited-but-subscribed session lingers (v4 ptyManager.get parity)"
    );
}

// === P4.D337 (bug 255): the screen model, resize, and the exit geometry ===

#[tokio::test(flavor = "multi_thread")]
async fn a_full_screen_program_renders_its_screen() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());
    assert_eq!((meta.cols, meta.rows), (Some(80), Some(24)));

    assert!(mgr.write(&meta.id, "printf '\\033[?1049h\\033[HTUI'\n"));
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2
                .render_screen(&id)
                .is_some_and(|r| r.buffer == ScreenBufferKind::Alternate),
            Duration::from_secs(10),
        )
        .await,
        "the alternate screen took over"
    );
    let r = mgr.render_screen(&meta.id).unwrap();
    assert!(r.lines[0].starts_with("TUI"), "{:?}", r.lines);
    assert!(mgr.render_screen("does-not-exist").is_none());
    let _ = mgr.write(&meta.id, "printf '\\033[?1049l'; exit 0\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn resize_moves_the_screen_and_the_meta_and_the_exit_stamps_it() {
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let meta = spawn_or_skip!(mgr, sh_opts());
    let row = read_row(&inst.db, &meta.id).unwrap();
    assert_eq!(
        (row.cols, row.rows),
        (Some(80), Some(24)),
        "spawn-time geometry"
    );

    mgr.resize(&meta.id, 100, 30);
    let r = mgr.render_screen(&meta.id).unwrap();
    assert_eq!((r.cols, r.rows), (100, 30));
    let m = mgr.session_meta(&meta.id).unwrap();
    assert_eq!((m.cols, m.rows), (Some(100), Some(30)));

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
    let row = read_row(&inst.db, &meta.id).unwrap();
    assert_eq!(
        (row.cols, row.rows),
        (Some(100), Some(30)),
        "the exit stamp wrote the last resize"
    );
}

/// The process-wide capture (the flush runs on runtime worker threads, so a
/// thread-scoped one cannot see it). Lines are filtered by session id, so the
/// binary's parallel tests do not disturb each other.
fn global_lines() -> &'static std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    use tracing_subscriber::layer::SubscriberExt;
    static LOGS: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<String>>>> =
        std::sync::OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(quilltap_core::test_support::CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn lines_with(needle: &str, session_id: &str) -> Vec<String> {
    global_lines()
        .lock()
        .unwrap()
        .iter()
        .filter(|l| l.contains(needle) && l.contains(&format!("sessionId={session_id}")))
        .cloned()
        .collect()
}

/// v4's DEBUG `[PTY] Ariel summary taken` `{ sessionId, reason, kind, chars }`
/// (+ the `pty-manager` child binding); a flush with nothing pending logs
/// nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_summary_taken_line_and_its_silence() {
    global_lines();
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_millis(200), Duration::from_secs(60));
    let meta = spawn_or_skip!(
        mgr,
        SpawnOptions {
            chat_id: "log-chat".to_string(),
            shell: Some("/bin/sh".to_string()),
            ..Default::default()
        }
    );
    assert!(mgr.write(&meta.id, "printf 'summary_marker\\n'\n"));
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2.ariel_pending(&id) == Some(true),
            Duration::from_secs(10)
        )
        .await
    );
    let mgr2 = Arc::clone(&mgr);
    let id = meta.id.clone();
    assert!(
        wait_until(
            move || mgr2.ariel_pending(&id) == Some(false),
            Duration::from_secs(10)
        )
        .await
    );
    tokio::time::sleep(Duration::from_millis(200)).await;

    let taken = lines_with("[PTY] Ariel summary taken", &meta.id);
    assert_eq!(taken.len(), 1, "{taken:#?}");
    let prefix = format!(
        "DEBUG quilltap::terminal [PTY] Ariel summary taken module=pty-manager sessionId={} \
         reason=idle kind=output chars=",
        meta.id
    );
    assert!(taken[0].starts_with(&prefix), "{}", taken[0]);

    // Silence: nothing pending → the kick's flush takes no summary.
    mgr.kick_all_for_chat("log-chat").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(lines_with("[PTY] Ariel summary taken", &meta.id).len(), 1);
}

/// v4's WARN `Failed to read transcript file` `{ sessionId, transcriptPath,
/// error }` (+ the `terminal-tool-handler` child binding), with Node's `fs`
/// message; a readable transcript logs nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_read_warn_and_its_silence() {
    global_lines();
    let inst = test_instance();
    let mgr = manager_with(&inst, Duration::from_secs(30), Duration::from_secs(120));
    let source = PtyScrollbackSource::new(Arc::clone(&mgr), inst.db.clone());
    let dir = tempfile::tempdir().unwrap();
    let present = dir.path().join("present.log");
    std::fs::write(&present, "$ ls\r\nfile\r\n$ ").unwrap();
    let missing = dir.path().join("missing.log");
    for (id, path) in [
        ("0f000000-0000-4000-8000-0000000000f1", &present),
        ("0f000000-0000-4000-8000-0000000000f2", &missing),
    ] {
        let path = path.to_string_lossy().into_owned();
        let id = id.to_string();
        inst.db
            .write(move |writers| {
                writers.main().terminal_sessions().create(
                    &terminal_sessions::TsCreate {
                        chat_id: CHAT_ID.to_string(),
                        label: None,
                        shell: "/bin/sh".to_string(),
                        cwd: "/tmp".to_string(),
                        started_at: "2026-01-01T00:00:00.000Z".to_string(),
                        exited_at: Some("2026-01-01T00:01:00.000Z".to_string()),
                        exit_code: Some(0.0),
                        transcript_path: Some(path),
                        cols: Some(20.0),
                        rows: Some(4.0),
                    },
                    &terminal_sessions::CreateOptions {
                        id,
                        created_at: "2026-01-01T00:00:00.000Z".to_string(),
                        updated_at: "2026-01-01T00:01:00.000Z".to_string(),
                    },
                )
            })
            .await
            .unwrap();
    }

    let read = source.read("0f000000-0000-4000-8000-0000000000f1");
    let r = read.rendering.expect("replayed");
    assert_eq!(
        (r.cols, r.rows, r.lines.clone()),
        (20, 4, vec!["$ ls".to_string(), "file".into(), "$".into()])
    );
    assert!(lines_with(
        "Failed to read transcript file",
        "0f000000-0000-4000-8000-0000000000f1"
    )
    .is_empty());

    let read = source.read("0f000000-0000-4000-8000-0000000000f2");
    assert!(read.rendering.is_none() && read.raw.is_empty());
    let warn = lines_with(
        "Failed to read transcript file",
        "0f000000-0000-4000-8000-0000000000f2",
    );
    assert_eq!(
        warn,
        [format!(
            "WARN quilltap::terminal Failed to read transcript file module=terminal-tool-handler \
             sessionId=0f000000-0000-4000-8000-0000000000f2 transcriptPath={0} \
             error=ENOENT: no such file or directory, open '{0}'",
            missing.display()
        )]
    );
}
// === end P4.D337 ===
