//! P4.D363 — `reindex-chat-message-fts-character-only-v1` (v4 `902c6efe9`,
//! #121) through the REAL `Host::start`, on four bases, capture-pinned with
//! its silence legs.
//!
//! (a) **pre-round** — a fresh instance booted once, then put back into the
//!     pre-round shape: `_ai` replaced by v4's `LEGACY_INSERT_TRIGGER`, and two
//!     Staff rows + one character line written so the legacy trigger INDEXES
//!     the Staff. The next boot logs v4's two INFO lines
//!     (`replacedTriggers=["chat_messages_fts_ai"]`, `indexedBefore=3`,
//!     `dropped=2`), stamps `Removed 2 Staff messages from the search index`,
//!     and the PHASE 3.65 reconcile after it says NOTHING — no "out of step"
//!     WARN, and the stale-replace DEBUG fires ONCE (the reindex's own ensure),
//!     not again from the reconcile. Global search no longer returns the Staff.
//!     That is round ruling R-6's whole point.
//! (b) the **second boot** of (a): `AlreadyCompleted`, silent, no new row.
//! (c) **v4-migrated** — current triggers and v4's ledger row: silent.
//! (d) **not-needed** — current triggers, counts equal, NO ledger row (v4's
//!     fresh-at-pin shape): silent, and NO reindex row written.
//!
//! The ensure runs inside `seed_built_ins`' `Db::write` (the writer thread),
//! so the capture is the process-wide subscriber.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::chat_message_fts_character_only_reindex::MIGRATION_ID;
use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const OPENING: &str = "Rebuilding chat message search index without Staff messages";
const CLOSING: &str = "Chat message search index rebuilt";
const CONTEXT: &str = "context=migrations.reindex-chat-message-fts-character-only";
const STALE: &str = "Replacing outdated chat message FTS triggers";
const OUT_OF_STEP: &str = "Message search index is out of step with the transcript; rebuilding";

/// v4's `LEGACY_INSERT_TRIGGER` (`chat-message-fts.test.ts` at `624d0d25c`).
const LEGACY_INSERT_TRIGGER: &str = r#"CREATE TRIGGER "chat_messages_fts_ai" AFTER INSERT ON "chat_messages"
  WHEN new."type" = 'message' AND new."role" IN ('USER','ASSISTANT') AND new."content" IS NOT NULL
BEGIN
  INSERT INTO "chat_messages_fts_map"("messageId") VALUES (new."id");
  INSERT INTO "chat_messages_fts"(rowid, content)
    VALUES ((SELECT "ftsId" FROM "chat_messages_fts_map" WHERE "messageId" = new."id"),
            qt_text(new."content"));
END"#;

const CHAT: &str = "c0000000-0000-4000-8000-000000000363";

fn capture() -> &'static Arc<Mutex<Vec<String>>> {
    static LOGS: OnceLock<Arc<Mutex<Vec<String>>>> = OnceLock::new();
    LOGS.get_or_init(|| {
        let logs = Arc::new(Mutex::new(Vec::new()));
        tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
        )
        .expect("this binary owns the global subscriber");
        logs
    })
}

fn config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config.set_display_zone(quilltap_core::host_zone::TimeZone::UTC);
    config
}

fn boot(base: &Path) -> Vec<String> {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    lines
}

fn open(data: &Path) -> Writer {
    Writer::open_writable(&data.join(MAIN), PEPPER).unwrap()
}

fn with(lines: &[String], needle: &str) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains(needle))
        .cloned()
        .collect()
}

fn reindex_rows(data: &Path) -> Vec<(i64, String)> {
    let w = open(data);
    let exists: bool = w
        .connection()
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='migrations_state'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !exists {
        return Vec::new();
    }
    let mut stmt = w
        .connection()
        .prepare("SELECT itemsAffected, message FROM migrations_state WHERE id = ?1")
        .unwrap();
    let rows = stmt
        .query_map([MIGRATION_ID], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

/// A provisioned instance booted ONCE (so the FTS objects exist).
fn booted_instance() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    boot(dir.path());
    (dir, data)
}

fn insert(w: &Writer, id: &str, content: &str, sender: Option<&str>, role: &str) {
    w.connection()
        .execute(
            r#"INSERT INTO "chat_messages" ("id","chatId","type","role","content","systemSender","createdAt")
               VALUES (?1, ?2, 'message', ?3, ?4, ?5, '2026-11-04T00:00:00.000Z')"#,
            rusqlite::params![id, CHAT, role, content, sender],
        )
        .unwrap();
}

/// The pre-round shape (a): legacy `_ai`, Staff rows indexed by it.
fn pre_round_instance() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, data) = booted_instance();
    let w = open(&data);
    let c = w.connection();
    c.execute("DELETE FROM migrations_state WHERE id = ?1", [MIGRATION_ID])
        .ok();
    c.execute_batch(r#"DROP TRIGGER "chat_messages_fts_ai""#)
        .unwrap();
    c.execute_batch(LEGACY_INSERT_TRIGGER).unwrap();
    c.execute(
        "INSERT INTO chats (id, userId, title, createdAt, updatedAt) VALUES (?1, 'u-1', 'T', ?2, ?2)",
        rusqlite::params![CHAT, "2026-11-04T00:00:00.000Z"],
    )
    .unwrap();
    insert(
        &w,
        "a0000000-0000-4000-8000-000000000001",
        "a dirigible recalled",
        Some("commonplaceBook"),
        "ASSISTANT",
    );
    insert(
        &w,
        "a0000000-0000-4000-8000-000000000002",
        "a dirigible announced",
        Some("host"),
        "ASSISTANT",
    );
    insert(
        &w,
        "a0000000-0000-4000-8000-000000000003",
        "a dirigible, said plainly",
        None,
        "USER",
    );
    // Self-check: the legacy trigger really indexed all three.
    let n: i64 = c
        .query_row(r#"SELECT COUNT(*) FROM "chat_messages_fts_map""#, [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(n, 3, "the plant must index the Staff rows");
    drop(w);
    (dir, data)
}

fn search_ids(data: &Path) -> Vec<String> {
    let w = open(data);
    let hits = w
        .chat_search()
        .search_messages_global(&[CHAT.to_string()], "dirigible", 100)
        .unwrap();
    hits.iter()
        .map(|h| h["messageId"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_reindexes_before_the_reconcile_then_stays_quiet() {
    let _g = SERIAL.lock().await;
    capture();
    let (dir, data) = pre_round_instance();

    // (a) the first post-round boot.
    let lines = boot(dir.path());
    let ours = with(&lines, CONTEXT);
    assert_eq!(ours.len(), 2, "{ours:#?}");
    assert!(ours[0].starts_with("INFO quilltap::db"), "{}", ours[0]);
    assert!(ours[0].contains(OPENING), "{}", ours[0]);
    assert!(
        ours[0].contains(r#"replacedTriggersJson=["chat_messages_fts_ai"]"#),
        "{}",
        ours[0]
    );
    assert!(ours[0].contains("indexedBefore=3"), "{}", ours[0]);
    assert!(ours[1].contains(CLOSING), "{}", ours[1]);
    for f in ["scanned=1", "indexed=1", "total=1", "dropped=2"] {
        assert!(ours[1].contains(f), "{f}: {}", ours[1]);
    }
    assert!(!ours[1].contains("durationMs"), "{}", ours[1]);
    // The reconcile after it: no WARN, and the stale DEBUG only ONCE (the
    // reindex's own ensure).
    assert!(with(&lines, OUT_OF_STEP).is_empty(), "{lines:#?}");
    assert_eq!(with(&lines, STALE).len(), 1, "{lines:#?}");
    assert_eq!(
        reindex_rows(&data),
        vec![(
            2,
            "Removed 2 Staff messages from the search index".to_string()
        )]
    );
    assert_eq!(
        search_ids(&data),
        vec!["a0000000-0000-4000-8000-000000000003".to_string()]
    );

    // (b) the second boot: AlreadyCompleted, silent.
    let lines = boot(dir.path());
    assert!(with(&lines, CONTEXT).is_empty(), "{lines:#?}");
    assert!(with(&lines, STALE).is_empty(), "{lines:#?}");
    assert!(with(&lines, OUT_OF_STEP).is_empty(), "{lines:#?}");
    assert_eq!(reindex_rows(&data).len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_v4_migrated_instance_is_already_completed() {
    let _g = SERIAL.lock().await;
    capture();
    let (dir, data) = booted_instance();
    {
        let w = open(&data);
        let c = w.connection();
        c.execute_batch(
            "CREATE TABLE IF NOT EXISTS \"migrations_state\" (\"id\" TEXT PRIMARY KEY, \"completedAt\" TEXT NOT NULL, \"quilltapVersion\" TEXT NOT NULL, \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT)",
        )
        .unwrap();
        c.execute(
            "INSERT INTO migrations_state (id, completedAt, quilltapVersion, itemsAffected, message) VALUES (?1, '2026-11-01T00:00:00.000Z', '4.12.0', 7, 'Removed 7 Staff messages from the search index')",
            [MIGRATION_ID],
        )
        .unwrap();
    }
    let lines = boot(dir.path());
    assert!(with(&lines, CONTEXT).is_empty(), "{lines:#?}");
    assert!(with(&lines, STALE).is_empty(), "{lines:#?}");
    assert_eq!(
        reindex_rows(&data),
        vec![(
            7,
            "Removed 7 Staff messages from the search index".to_string()
        )],
        "v4's row is left as v4 wrote it"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_healthy_unstamped_instance_is_not_needed_and_stamps_nothing() {
    let _g = SERIAL.lock().await;
    capture();
    // A fresh instance after its first boot: current triggers, counts equal,
    // no reindex row (the first boot was `DependencyPending` — the index did
    // not exist yet). v4's fresh-at-pin shape.
    let (dir, data) = booted_instance();
    assert!(
        reindex_rows(&data).is_empty(),
        "the first boot must stamp nothing"
    );
    let lines = boot(dir.path());
    assert!(with(&lines, CONTEXT).is_empty(), "{lines:#?}");
    assert!(with(&lines, STALE).is_empty(), "{lines:#?}");
    assert!(reindex_rows(&data).is_empty(), "NotNeeded writes no row");
}
