//! P4.D315 Tier 1 item 8 — the chat wardrobe history's three boot ensures
//! through the REAL `Host::start`.
//!
//! (a) A PRE-ROUND instance (a fresh provision — v5 never provisions the
//!     table; v4's generateDDL never renders it) carrying dressed chats: one
//!     boot creates the table at its final 11-column shape with both UNIQUE
//!     indexes, seeds one current entry 0 per (chat, character) dated by the
//!     chat's `updatedAt`, stamps the TABLE and the SEED rows with v4's
//!     messages (the column step is "not needed" and unrecorded, as v4), logs
//!     two INFOs (no column INFO); a SECOND boot writes nothing and logs no
//!     history line.
//! (b) A v4-booted mode-B instance (the `83387d0e5` 10-column table + its
//!     ledger row + rows planted): the column is added and stamped, the seed
//!     stamped, the existing rows kept with `actorToolName` NULL, and the
//!     table's `sqlite_master.sql` is the ALTERed text.
//! (c) A fresh `setup` instance: the table + seed stamps over zero chats
//!     (`Seeded 0 outfit record(s) across 0 chat(s)`).
//! (d) A mode-E′ instance (a dressed chat with a NULL `updatedAt` — the
//!     `chats` schema's `NOT NULL` lifted by a `writable_schema` plant, the
//!     only way a NULL lands on a generateDDL `chats`): the boot SUCCEEDS with
//!     the ERROR line, the seed unstamped; the next boot retries and fails
//!     again with the same line (R-2 / R-3).
//!
//! The three INFO lines and the seed ERROR are v5 literals (v4's migration
//! lines are NO-PORT, `db::chat_wardrobe_history_repair`'s header) — pinned
//! here as v5's boot contract, NOT oracle-pinned.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::chat_wardrobe_history::CHAT_WARDROBE_HISTORY_DDL;
use quilltap_core::db::chat_wardrobe_history_repair::{
    SEED_MIGRATION_ID, TABLE_MIGRATION_ID, TOOL_NAME_MIGRATION_ID,
};
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

/// The history lines a boot logged (ours only — other ensures log too).
fn history_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| l.contains("chat wardrobe history") || l.contains("chat-wardrobe-history"))
        .cloned()
        .collect()
}

#[allow(clippy::type_complexity)]
struct Snapshot {
    master: Vec<(String, String)>,
    rows: Vec<(
        String,
        String,
        i64,
        String,
        String,
        Option<String>,
        Option<String>,
        i64,
        String,
    )>,
    ledger: Vec<(String, i64, String)>,
}

fn snapshot(data: &Path) -> Snapshot {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    let master: Vec<(String, String)> = c
        .prepare(
            "SELECT name, sql FROM sqlite_master WHERE tbl_name = 'chat_wardrobe_history' \
             AND sql IS NOT NULL ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let rows = if master.is_empty() {
        Vec::new()
    } else {
        c.prepare(
            "SELECT chatId, characterId, entryIndex, slots, source, actorCharacterId, \
             actorToolName, isCurrent, createdAt FROM chat_wardrobe_history \
             ORDER BY chatId, characterId, entryIndex",
        )
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    };
    let ledger = c
        .prepare(
            "SELECT id, itemsAffected, message FROM migrations_state \
             WHERE id IN (?1, ?2, ?3) ORDER BY id",
        )
        .unwrap()
        .query_map(
            [
                TABLE_MIGRATION_ID,
                TOOL_NAME_MIGRATION_ID,
                SEED_MIGRATION_ID,
            ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Snapshot {
        master,
        rows,
        ledger,
    }
}

const A: &str = "11111111-1111-4111-8111-11111111111a";
const B: &str = "11111111-1111-4111-8111-11111111111b";
const CHAT_1: &str = "c1c1c1c1-0000-4000-8000-000000000001";
const CHAT_2: &str = "c2c2c2c2-0000-4000-8000-000000000002";

fn dress(data: &Path, chats: &[(&str, &str, String)]) {
    for (id, updated, outfit) in chats {
        exec(
            data,
            &format!(
                "INSERT INTO chats (id, userId, title, createdAt, updatedAt, equippedOutfit) \
                 VALUES ('{id}', 'u1', '{id}', '{updated}', '{updated}', '{outfit}');"
            ),
        );
    }
}

fn dressed() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            CHAT_1,
            "2026-01-01T00:00:00.000Z",
            format!(r#"{{"{A}":{{"top":["coat"]}},"{B}":null}}"#),
        ),
        (CHAT_2, "2026-03-01T00:00:00.000Z", "nope".to_string()),
    ]
}

const EMPTY: &str = r#"{"top":[],"bottom":[],"footwear":[],"accessories":[],"hair":[]}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_is_created_seeded_and_stamped_once() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    dress(&data, &dressed());

    let lines = boot(dir.path());
    let snap = snapshot(&data);
    assert_eq!(
        snap.master,
        vec![
            (
                "chat_wardrobe_history".to_string(),
                CHAT_WARDROBE_HISTORY_DDL[0].replacen(" IF NOT EXISTS", "", 1)
            ),
            (
                "idx_chat_wardrobe_history_current".to_string(),
                CHAT_WARDROBE_HISTORY_DDL[2].replacen(" IF NOT EXISTS", "", 1)
            ),
            (
                "idx_chat_wardrobe_history_entry".to_string(),
                CHAT_WARDROBE_HISTORY_DDL[1].replacen(" IF NOT EXISTS", "", 1)
            ),
        ]
    );
    let seeded = |character: &str, slots: &str| {
        (
            CHAT_1.to_string(),
            character.to_string(),
            0i64,
            slots.to_string(),
            "seeded".to_string(),
            None,
            None,
            1i64,
            "2026-01-01T00:00:00.000Z".to_string(),
        )
    };
    assert_eq!(
        snap.rows,
        vec![
            seeded(
                A,
                r#"{"top":["coat"],"bottom":[],"footwear":[],"accessories":[],"hair":[]}"#
            ),
            seeded(B, EMPTY),
        ]
    );
    assert_eq!(
        snap.ledger,
        vec![
            (
                TABLE_MIGRATION_ID.into(),
                1,
                "Created chat_wardrobe_history table".into()
            ),
            (
                SEED_MIGRATION_ID.into(),
                2,
                "Seeded 2 outfit record(s) across 2 chat(s)".into()
            ),
        ]
    );
    assert_eq!(
        history_lines(&lines),
        vec![
            format!("INFO quilltap::boot Created the chat wardrobe history migrationId={TABLE_MIGRATION_ID}"),
            format!(
                "INFO quilltap::boot Seeded the chat wardrobe history from current outfits migrationId={SEED_MIGRATION_ID} entriesSeeded=2 chatsScanned=2 pairsSkipped=0"
            ),
        ]
    );

    // The second boot writes nothing and logs nothing of ours.
    let lines = boot(dir.path());
    let again = snapshot(&data);
    assert_eq!(again.master, snap.master);
    assert_eq!(again.rows, snap.rows);
    assert_eq!(again.ledger, snap.ledger);
    assert!(history_lines(&lines).is_empty(), "{lines:#?}");
}

/// The `83387d0e5` pre-column table text (`:18-29`), byte-exact.
const PRE_COLUMN_TABLE: &str = "CREATE TABLE IF NOT EXISTS \"chat_wardrobe_history\" (\n    \"id\" TEXT PRIMARY KEY,\n    \"chatId\" TEXT NOT NULL REFERENCES \"chats\"(\"id\") ON DELETE CASCADE,\n    \"characterId\" TEXT NOT NULL,\n    \"entryIndex\" INTEGER NOT NULL,\n    \"slots\" TEXT NOT NULL,\n    \"slotsHash\" TEXT NOT NULL,\n    \"source\" TEXT NOT NULL,\n    \"actorCharacterId\" TEXT,\n    \"isCurrent\" INTEGER NOT NULL DEFAULT 0,\n    \"createdAt\" TEXT NOT NULL\n  )";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_v4_booted_pre_column_instance_gains_the_column() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    dress(&data, &dressed());
    exec(
        &data,
        &format!(
            "{PRE_COLUMN_TABLE}; {}; {}; \
             CREATE TABLE IF NOT EXISTS \"migrations_state\" (\"id\" TEXT PRIMARY KEY, \
               \"completedAt\" TEXT NOT NULL, \"quilltapVersion\" TEXT NOT NULL, \
               \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT); \
             INSERT INTO migrations_state VALUES ('{TABLE_MIGRATION_ID}', 'x', '4.11.0', 1, 'Created chat_wardrobe_history table'); \
             INSERT INTO chat_wardrobe_history (id, chatId, characterId, entryIndex, slots, slotsHash, source, actorCharacterId, isCurrent, createdAt) \
               VALUES ('99999999-9999-4999-8999-999999999999', '{CHAT_1}', '{A}', 0, '{EMPTY}', 'h', 'pascal', '{A}', 1, '2026-01-05T00:00:00.000Z');",
            CHAT_WARDROBE_HISTORY_DDL[1], CHAT_WARDROBE_HISTORY_DDL[2],
        ),
    );
    let lines = boot(dir.path());
    let snap = snapshot(&data);
    assert_eq!(
        snap.master[0].1,
        "CREATE TABLE \"chat_wardrobe_history\" (\n    \"id\" TEXT PRIMARY KEY,\n    \"chatId\" TEXT NOT NULL REFERENCES \"chats\"(\"id\") ON DELETE CASCADE,\n    \"characterId\" TEXT NOT NULL,\n    \"entryIndex\" INTEGER NOT NULL,\n    \"slots\" TEXT NOT NULL,\n    \"slotsHash\" TEXT NOT NULL,\n    \"source\" TEXT NOT NULL,\n    \"actorCharacterId\" TEXT,\n    \"isCurrent\" INTEGER NOT NULL DEFAULT 0,\n    \"createdAt\" TEXT NOT NULL\n  , \"actorToolName\" TEXT)"
    );
    // The planted pascal row kept, `actorToolName` NULL; B seeded beside it.
    assert_eq!(snap.rows.len(), 2);
    assert_eq!(snap.rows[0].4, "pascal");
    assert_eq!(snap.rows[0].6, None);
    assert_eq!(snap.rows[1].1, B);
    assert_eq!(
        snap.ledger,
        vec![
            (
                TABLE_MIGRATION_ID.into(),
                1,
                "Created chat_wardrobe_history table".into()
            ),
            (
                TOOL_NAME_MIGRATION_ID.into(),
                1,
                "Added actorToolName column to chat_wardrobe_history".into()
            ),
            (
                SEED_MIGRATION_ID.into(),
                1,
                "Seeded 1 outfit record(s) across 2 chat(s)".into()
            ),
        ]
    );
    assert_eq!(
        history_lines(&lines),
        vec![
            format!(
                "INFO quilltap::boot Fitted the chat wardrobe history with its tool-name column migrationId={TOOL_NAME_MIGRATION_ID}"
            ),
            format!(
                "INFO quilltap::boot Seeded the chat wardrobe history from current outfits migrationId={SEED_MIGRATION_ID} entriesSeeded=1 chatsScanned=2 pairsSkipped=1"
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_stamps_the_table_and_the_empty_seed() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    boot(dir.path());
    let snap = snapshot(&data);
    assert_eq!(snap.master.len(), 3);
    assert!(snap.rows.is_empty());
    assert_eq!(
        snap.ledger,
        vec![
            (
                TABLE_MIGRATION_ID.into(),
                1,
                "Created chat_wardrobe_history table".into()
            ),
            (
                SEED_MIGRATION_ID.into(),
                0,
                "Seeded 0 outfit record(s) across 0 chat(s)".into()
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_null_updated_at_fails_the_seed_and_the_boot_continues() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    dress(&data, &dressed());
    // Lift `chats.updatedAt`'s NOT NULL (mode E′'s hand-built `chats`).
    exec(
        &data,
        "PRAGMA writable_schema = ON; \
         UPDATE sqlite_master SET sql = replace(sql, '\"updatedAt\" TEXT NOT NULL', '\"updatedAt\" TEXT') \
           WHERE type = 'table' AND name = 'chats'; \
         PRAGMA writable_schema = OFF;",
    );
    exec(
        &data,
        &format!("UPDATE chats SET updatedAt = NULL WHERE id = '{CHAT_1}';"),
    );
    let line = format!(
        "ERROR quilltap::boot Failed to seed the chat wardrobe history; it will be retried next boot migrationId={SEED_MIGRATION_ID} error=NOT NULL constraint failed: chat_wardrobe_history.createdAt"
    );
    for boot_no in 0..2 {
        let lines = boot(dir.path());
        let ours = history_lines(&lines);
        assert!(ours.contains(&line), "boot {boot_no}: {lines:#?}");
        let snap = snapshot(&data);
        assert_eq!(snap.master.len(), 3, "the table intact");
        assert!(snap.rows.is_empty());
        assert_eq!(
            snap.ledger,
            vec![(
                TABLE_MIGRATION_ID.into(),
                1,
                "Created chat_wardrobe_history table".into()
            )],
            "the seed unstamped"
        );
    }
}
