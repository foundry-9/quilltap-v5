//! End-to-end: a REAL PTY session over the v4-baked Ariel fixture — output
//! flows through the idle flush driver into a REAL Ariel `chat_messages` row,
//! and subscribers get the `chat-update` nudge (the branch the plain PTY
//! tests can't reach: they carry no `chats` table, so the writer no-ops).
//!
//! The announcement bytes themselves are byte-verified against v4 by the
//! `ariel_writers_tier3_equivalence` differential; this proves the HOST
//! composition — reader thread → screen model → idle timer → summary → post →
//! broadcast — lands the row and the frame end to end, with the RENDERED text
//! (bug 255): the echoed command and its output as their own lines, no escape
//! bytes. A second beat drives `kick_all_for_chat` against a pending timer
//! flush — the per-session flush chain posts the summary ONCE (P4.D337 R-8).
//!
//! Needs the differential's fixture (skips when unset):
//!   QT_FIXTURE_ARIEL_MAIN=/tmp/qt-ariel-main.db \
//!     cargo test -p quilltap-host --test terminal_ariel_e2e

#![cfg(unix)]

use std::time::Duration;

use quilltap_core::db::dump_table_json_conn;
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_host::terminal::protocol::{ChatUpdateReason, WsServerMessage};
use quilltap_host::terminal::{SpawnOptions, TerminalManager, TerminalManagerConfig};
use serde_json::Value;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
    chat: ChatSpec,
}

#[derive(serde::Deserialize)]
struct ChatSpec {
    id: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn ariel_flush_posts_row_and_broadcasts_chat_update() {
    let main_fixture = match std::env::var("QT_FIXTURE_ARIEL_MAIN") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_FIXTURE_ARIEL_MAIN to the Ariel fixture .db (see header).");
            return;
        }
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/ariel-writers-tier3.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");

    let dir = tempfile::tempdir().expect("tempdir");
    let main_work = dir.path().join("ariel-e2e-main.db");
    std::fs::copy(&main_fixture, &main_work).expect("copy fixture");

    let db = Db::open(
        DbPaths {
            main: main_work,
            mount_index: None,
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture copy");

    let files_dir = dir.path().join("files");
    let logs_dir = dir.path().join("logs");
    std::fs::create_dir_all(&files_dir).unwrap();
    std::fs::create_dir_all(&logs_dir).unwrap();

    let mut cfg =
        TerminalManagerConfig::new(db.clone(), files_dir, logs_dir, dir.path().to_path_buf());
    cfg.ariel_flush_idle = Duration::from_millis(300);
    cfg.ariel_flush_max_age = Duration::from_secs(60);
    let mgr = TerminalManager::new(cfg);

    let meta = match mgr
        .spawn(SpawnOptions {
            chat_id: spec.chat.id.clone(),
            shell: Some("/bin/sh".to_string()),
            ..Default::default()
        })
        .await
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("SKIP: PTY spawn unavailable in this environment: {e}");
            return;
        }
    };

    let mut sub = mgr.subscribe(&meta.id).expect("subscribe");
    assert!(mgr.write(&meta.id, "printf '%s %s\\n' live ariel_e2e_payload\n"));

    // Frames arrive: output echo/prompt, then — after the 300 ms idle window —
    // the chat-update nudge proving the writer actually posted.
    let mut saw_chat_update = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        let msg = match tokio::time::timeout_at(deadline, sub.rx.recv()).await {
            Ok(Some(m)) => m,
            _ => break,
        };
        if let WsServerMessage::ChatUpdate { chat_id, reason } = msg {
            assert_eq!(chat_id, spec.chat.id);
            assert_eq!(reason, Some(ChatUpdateReason::ArielTerminalOutput));
            saw_chat_update = true;
            break;
        }
    }
    assert!(saw_chat_update, "subscriber got the chat-update nudge");

    // The Ariel row landed: an ASSISTANT message from systemSender 'ariel'
    // whose body embeds the session id and carries the cleaned output.
    let dump = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "content"))
        .expect("dump chat_messages");
    let rows = dump["rows"].as_array().expect("rows");
    let ariel_row = rows
        .iter()
        .find(|r| {
            r.get("content")
                .and_then(Value::as_str)
                .map(|c| c.contains("ariel_e2e_payload"))
                .unwrap_or(false)
        })
        .expect("Ariel terminal-output row landed");
    assert_eq!(ariel_row["systemSender"], "ariel");
    assert_eq!(ariel_row["systemKind"], "terminal-output-idle");
    assert_eq!(ariel_row["role"], "ASSISTANT");
    let content = ariel_row["content"].as_str().unwrap();
    assert!(content.contains(&format!("<!-- terminalSessionId:{} -->", meta.id)));
    // Rendered, not stripped: the output is a line of its own, and no escape
    // byte survives.
    assert!(
        content.contains("\nlive ariel_e2e_payload\n"),
        "the output line, rendered: {content:?}"
    );
    assert!(!content.contains('\u{1b}'), "{content:?}");

    let _ = mgr.write(&meta.id, "exit\n");
}

/// P4.D337 R-8: a kick racing a pending timer flush posts ONE summary — the
/// second flush waits for the first and finds nothing pending.
#[tokio::test(flavor = "multi_thread")]
async fn a_kick_racing_a_timer_flush_posts_one_summary() {
    let Ok(main_fixture) = std::env::var("QT_FIXTURE_ARIEL_MAIN") else {
        eprintln!("SKIP: set QT_FIXTURE_ARIEL_MAIN to the Ariel fixture .db (see header).");
        return;
    };
    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../harness/oracle/fixtures/ariel-writers-tier3.json"),
        )
        .expect("read spec"),
    )
    .expect("parse spec");
    let dir = tempfile::tempdir().expect("tempdir");
    let main_work = dir.path().join("ariel-e2e-race.db");
    std::fs::copy(&main_fixture, &main_work).expect("copy fixture");
    let db = Db::open(
        DbPaths {
            main: main_work,
            mount_index: None,
            llm_logs: None,
        },
        &spec.test_pepper_base64,
    )
    .expect("open fixture copy");
    let files_dir = dir.path().join("files");
    let logs_dir = dir.path().join("logs");
    std::fs::create_dir_all(&files_dir).unwrap();
    std::fs::create_dir_all(&logs_dir).unwrap();
    let mut cfg =
        TerminalManagerConfig::new(db.clone(), files_dir, logs_dir, dir.path().to_path_buf());
    cfg.ariel_flush_idle = Duration::from_millis(400);
    cfg.ariel_flush_max_age = Duration::from_secs(60);
    let mgr = TerminalManager::new(cfg);
    let meta = match mgr
        .spawn(SpawnOptions {
            chat_id: spec.chat.id.clone(),
            shell: Some("/bin/sh".to_string()),
            ..Default::default()
        })
        .await
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("SKIP: PTY spawn unavailable in this environment: {e}");
            return;
        }
    };
    assert!(mgr.write(&meta.id, "printf '%s\\n' race_payload\n"));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while mgr.ariel_pending(&meta.id) != Some(true) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "output never pending"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Let the idle window nearly lapse, then kick: the two flushes overlap.
    tokio::time::sleep(Duration::from_millis(380)).await;
    mgr.kick_all_for_chat(&spec.chat.id).await;
    tokio::time::sleep(Duration::from_millis(800)).await;

    let dump = db
        .read_main(|conn| dump_table_json_conn(conn, "chat_messages", "content"))
        .expect("dump chat_messages");
    let summaries = dump["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            r["systemKind"]
                .as_str()
                .is_some_and(|k| k.starts_with("terminal-output-"))
                && r["content"].as_str().is_some_and(|c| c.contains(&meta.id))
        })
        .count();
    assert_eq!(summaries, 1, "one summary for the race");
}
