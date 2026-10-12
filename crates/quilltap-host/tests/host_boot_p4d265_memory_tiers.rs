//! P4.D265 Tier 2 item 16 — the memory-tiers boot ensure (v4 `d58548051`,
//! `add-memory-tiers-v1`) through the REAL `Host::start`.
//!
//! (a) A PRE-ROUND instance (a fresh provision with this round's four
//!     `memories` columns, their index and `chats.otherExtractionWatermarkMessageId`
//!     taken away — every instance `setup` before P4.D265), with a memory and a
//!     chat: one boot APPENDS the four columns in the migration's text
//!     (`TEXT DEFAULT NULL` on the nullable two), creates the index and the chat
//!     column, logs v5's ONE `Fitted the memory tiers` line and NONE of v4's
//!     migration lines, and writes NO `migrations_state` row (R-A); a SECOND boot
//!     is an exact no-op and silent.
//! (b) A planted NULL `tier` is healed on the next boot (R-A's every-boot heal).
//! (c) A fresh `setup` instance: both new indexes from provisioning, the
//!     columns in generateDDL's slots, the ensure a silent no-op.
//! (d) A v4-migrated plant (the appended columns + the index + v4's own ledger
//!     row): a silent no-op, the ledger untouched.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::memory_tiers_repair::{CHAT_WATERMARK_COLUMN, INDEX_NAME, MIGRATION_ID};
use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const MAIN: &str = "quilltap.db";

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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
    config
}

fn exec(data: &Path, sql: &str) {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    w.connection()
        .execute_batch(sql)
        .unwrap_or_else(|e| panic!("plant {sql:?}: {e}"));
}

fn boot(base: &Path) -> Vec<String> {
    capture().lock().unwrap().clear();
    let host = Host::start(config(base)).expect("the boot");
    let lines = capture().lock().unwrap().clone();
    drop(host);
    lines
}

/// `(column (name, type, dflt), …)` for a table.
fn columns(data: &Path, table: &str) -> Vec<(String, String, Option<String>)> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let rows = w
        .connection()
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap()
        .query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(4)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn master(data: &Path) -> Vec<(String, Option<String>)> {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let rows = w
        .connection()
        .prepare("SELECT name, sql FROM sqlite_master WHERE tbl_name IN ('memories', 'chats') ORDER BY name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn ledger_rows(data: &Path) -> i64 {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    let has = c
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'migrations_state'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !has {
        return 0;
    }
    c.query_row(
        "SELECT COUNT(*) FROM migrations_state WHERE id = ?1",
        [MIGRATION_ID],
        |r| r.get(0),
    )
    .unwrap()
}

/// A fresh provision with this round's schema moves taken away, plus a memory
/// and a chat.
fn derive_pre_round(data: &Path) {
    provision_fresh_instance(data, PEPPER).expect("provision");
    exec(
        data,
        &format!(
            "DROP INDEX \"{INDEX_NAME}\"; \
             ALTER TABLE memories DROP COLUMN tier; \
             ALTER TABLE memories DROP COLUMN supersededById; \
             ALTER TABLE memories DROP COLUMN consolidatedFrom; \
             ALTER TABLE memories DROP COLUMN consolidatedAt; \
             ALTER TABLE chats DROP COLUMN \"{CHAT_WATERMARK_COLUMN}\"; \
             INSERT INTO memories (id, characterId, content, summary, createdAt, updatedAt) \
               VALUES ('m1', 'c1', 'x', 'y', 't', 't'); \
             INSERT INTO chats (id, userId, title, createdAt, updatedAt) \
               VALUES ('chat1', 'u1', 't', 't', 't');"
        ),
    );
}

const FITTED: &str = "Fitted the memory tiers";
/// v4's migration lines (NO-PORT, R-J) — none may ever appear.
const V4_LINES: [&str; 5] = [
    "Added memories.",
    "Created idx_memories_character_tier",
    "Stamped NULL-tier memories hot",
    "Added chats.otherExtractionWatermarkMessageId column",
    "Memory-tiers migration failed",
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_is_fitted_once_unstamped_and_in_the_migrations_text() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);

    let lines = boot(dir.path());
    let mem = columns(&data, "memories");
    assert_eq!(
        mem[mem.len() - 4..],
        [
            ("tier".into(), "TEXT".into(), Some("'hot'".into())),
            ("supersededById".into(), "TEXT".into(), Some("NULL".into())),
            (
                "consolidatedFrom".into(),
                "TEXT".into(),
                Some("'[]'".into())
            ),
            ("consolidatedAt".into(), "TEXT".into(), Some("NULL".into())),
        ],
        "APPENDED in the migration's text"
    );
    assert_eq!(
        columns(&data, "chats").last().unwrap(),
        &(
            CHAT_WATERMARK_COLUMN.to_string(),
            "TEXT".to_string(),
            Some("NULL".to_string())
        )
    );
    assert!(master(&data).iter().any(|(n, _)| n == INDEX_NAME));
    let fitted: Vec<&String> = lines.iter().filter(|l| l.contains(FITTED)).collect();
    assert_eq!(
        fitted,
        ["INFO quilltap::boot Fitted the memory tiers migrationId=add-memory-tiers-v1 columnsAdded=4 indexCreated=true healedRows=0 chatColumnAdded=true"],
        "{lines:#?}"
    );
    for v4 in V4_LINES {
        assert!(
            !lines.iter().any(|l| l.contains(v4)),
            "v4's NO-PORT line {v4:?}: {lines:#?}"
        );
    }
    assert!(
        !lines
            .iter()
            .any(|l| l.contains("Failed to backfill a migration-created index")),
        "the ensure runs BEFORE the index backfill: {lines:#?}"
    );
    assert_eq!(ledger_rows(&data), 0, "UNSTAMPED (R-A)");
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        let (tier, from): (String, String) = w
            .connection()
            .query_row(
                "SELECT tier, consolidatedFrom FROM memories WHERE id = 'm1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((tier.as_str(), from.as_str()), ("hot", "[]"));
    }

    // The second boot: an exact no-op, silent.
    let before = master(&data);
    let lines = boot(dir.path());
    assert_eq!(
        master(&data),
        before,
        "sqlite_master moved on the second boot"
    );
    assert!(!lines.iter().any(|l| l.contains(FITTED)), "{lines:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_planted_null_tier_is_healed_on_the_next_boot() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    boot(dir.path());
    exec(&data, "UPDATE memories SET tier = NULL WHERE id = 'm1'");
    let lines = boot(dir.path());
    assert!(
        lines.iter().any(|l| l
            == "INFO quilltap::boot Fitted the memory tiers migrationId=add-memory-tiers-v1 columnsAdded=0 indexCreated=false healedRows=1 chatColumnAdded=false"),
        "{lines:#?}"
    );
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let tier: String = w
        .connection()
        .query_row("SELECT tier FROM memories WHERE id = 'm1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(tier, "hot");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_carries_both_indexes_and_the_ensure_is_silent() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    let lines = boot(dir.path());
    assert!(!lines.iter().any(|l| l.contains(FITTED)), "{lines:#?}");
    assert!(master(&data).iter().any(|(n, _)| n == INDEX_NAME));
    let mount = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
    let nocase: String = mount
        .connection()
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'idx_doc_mount_points_name_nocase'",
            [],
            |r| r.get(0),
        )
        .expect("the NOCASE store-name index from provisioning");
    assert!(nocase.contains("COLLATE NOCASE"), "{nocase}");
    let mem = columns(&data, "memories");
    let at = mem.iter().position(|c| c.0 == "tier").unwrap();
    assert_eq!(mem[at - 1].0, "reinforcedImportance", "generateDDL's slot");
    assert_eq!(ledger_rows(&data), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_v4_migrated_instance_is_a_silent_no_op() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    // v4's migration, by its own text, then its ledger row.
    exec(
        &data,
        &format!(
            "ALTER TABLE \"memories\" ADD COLUMN \"tier\" TEXT DEFAULT 'hot'; \
             ALTER TABLE \"memories\" ADD COLUMN \"supersededById\" TEXT DEFAULT NULL; \
             ALTER TABLE \"memories\" ADD COLUMN \"consolidatedFrom\" TEXT DEFAULT '[]'; \
             ALTER TABLE \"memories\" ADD COLUMN \"consolidatedAt\" TEXT DEFAULT NULL; \
             CREATE INDEX IF NOT EXISTS \"{INDEX_NAME}\" ON \"memories\" (\"characterId\", \"tier\"); \
             ALTER TABLE \"chats\" ADD COLUMN \"{CHAT_WATERMARK_COLUMN}\" TEXT DEFAULT NULL; \
             CREATE TABLE IF NOT EXISTS \"migrations_state\" (\"id\" TEXT PRIMARY KEY, \
               \"completedAt\" TEXT NOT NULL, \"quilltapVersion\" TEXT NOT NULL, \
               \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT); \
             INSERT INTO migrations_state VALUES ('{MIGRATION_ID}', 'x', '4.10.0', 6, 'Memory tiers in place (6 changes)');"
        ),
    );
    // The tier surface only: the planted ledger table wakes OTHER boot steps
    // that append their own `chats` columns, which is not this ensure's doing.
    let surface = |data: &Path| {
        let index = master(data).into_iter().find(|(n, _)| n == INDEX_NAME);
        let watermark: Vec<_> = columns(data, "chats")
            .into_iter()
            .filter(|c| c.0 == CHAT_WATERMARK_COLUMN)
            .collect();
        (columns(data, "memories"), index, watermark)
    };
    let before = surface(&data);
    let lines = boot(dir.path());
    assert!(!lines.iter().any(|l| l.contains(FITTED)), "{lines:#?}");
    assert_eq!(surface(&data), before);
    assert_eq!(ledger_rows(&data), 1, "v4's row untouched");
}
