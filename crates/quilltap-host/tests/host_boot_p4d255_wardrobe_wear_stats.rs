//! P4.D255 Tier 2 item 22 — the wardrobe wear ledger's boot ensures through the
//! REAL `Host::start`.
//!
//! (a) A PRE-ROUND instance (a fresh provision with this round's
//!     `wardrobe_wear_stats` and `chat_settings."wardrobeImageSettings"` taken
//!     away — every instance `setup` before P4.D255) carrying chats with
//!     outfits: one boot creates the table in v4's MIGRATION text with its
//!     three indexes, seeds exactly what `wears_from_equipped_outfit` counts,
//!     stamps BOTH `migrations_state` rows with v4's messages, and appends the
//!     settings column with the one-key default; a SECOND boot writes nothing
//!     (same rows, same ledger, same `sqlite_master`).
//! (b) The cross-app arm: v4 already booted the instance (its two ledger rows
//!     planted, the table absent is unreachable from v4 — here it is present
//!     and EMPTY, as v4's boot leaves an instance whose chats had no outfits
//!     before): zero rows seeded, no new ledger row.
//! (c) A fresh `setup` instance: the table from provisioning (generateDDL's
//!     text), ZERO rows, and the first boot stamps the SEED row alone — v4's
//!     real first boot runs the seed over zero chats (`Credited 0 wear(s)
//!     across 0 chat(s)`, measured at the `f5e953a3f` pin); the table row is
//!     v4's only when its migration RAN, which it does not here.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use quilltap_core::db::wardrobe_wear_stats_repair::{
    wears_from_equipped_outfit, SEED_MIGRATION_ID, TABLE_MIGRATION_ID,
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

/// `(sqlite_master rows for the ledger, ledger rows, migrations_state rows)`.
type Snapshot = (
    Vec<(String, String)>,
    Vec<(String, Option<String>, i64, String, String, Option<String>)>,
    Vec<(String, i64, String)>,
);

fn snapshot(data: &Path) -> Snapshot {
    let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
    let c = w.connection();
    let master = c
        .prepare(
            "SELECT name, sql FROM sqlite_master WHERE tbl_name = 'wardrobe_wear_stats' \
             AND sql IS NOT NULL ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let rows = c
        .prepare(
            "SELECT itemId, wearerCharacterId, wearCount, firstWornAt, lastWornAt, lastWornChatId \
             FROM wardrobe_wear_stats ORDER BY rowid",
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
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let ledger = c
        .prepare(
            "SELECT id, itemsAffected, message FROM migrations_state \
             WHERE id IN (?1, ?2) ORDER BY id",
        )
        .unwrap()
        .query_map([TABLE_MIGRATION_ID, SEED_MIGRATION_ID], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (master, rows, ledger)
}

const A: &str = "11111111-1111-4111-8111-11111111111a";
const B: &str = "11111111-1111-4111-8111-11111111111b";

fn outfits() -> Vec<(&'static str, &'static str, String)> {
    vec![
        (
            "chat-1",
            "2026-01-01T00:00:00.000Z",
            format!(r#"{{"{A}":{{"top":["coat","shirt"]}},"{B}":{{"top":["coat"]}}}}"#),
        ),
        (
            "chat-2",
            "2026-03-01T00:00:00.000Z",
            format!(r#"{{"{A}":{{"top":["coat"],"accessories":["coat"]}}}}"#),
        ),
    ]
}

/// A fresh provision with this round's two schema moves taken away, plus the
/// chats.
fn derive_pre_round(data: &Path) {
    provision_fresh_instance(data, PEPPER).expect("provision");
    exec(
        data,
        "DROP TABLE \"wardrobe_wear_stats\"; \
         ALTER TABLE \"chat_settings\" DROP COLUMN \"wardrobeImageSettings\";",
    );
    for (id, updated, outfit) in outfits() {
        exec(
            data,
            &format!(
                "INSERT INTO chats (id, userId, title, createdAt, updatedAt, equippedOutfit) \
                 VALUES ('{id}', 'u1', '{id}', '{updated}', '{updated}', '{outfit}');"
            ),
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pre_round_instance_is_created_seeded_and_stamped_once() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);

    let lines = boot(dir.path());
    let (master, rows, ledger) = snapshot(&data);
    let names: Vec<&str> = master.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "idx_wardrobe_wear_stats_createdAt",
            "idx_wardrobe_wear_stats_item_wearer",
            "idx_wardrobe_wear_stats_wearer",
            "wardrobe_wear_stats",
        ]
    );
    assert!(
        master[3]
            .1
            .contains("\"wearCount\" INTEGER NOT NULL DEFAULT 0"),
        "the migration's text: {}",
        master[3].1
    );
    let want: usize = outfits()
        .iter()
        .map(|(_, _, o)| wears_from_equipped_outfit(Some(o)).len())
        .sum();
    assert_eq!(want, 4);
    assert_eq!(rows.iter().map(|r| r.2).sum::<i64>(), want as i64);
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(
        ledger,
        vec![
            (
                TABLE_MIGRATION_ID.into(),
                1,
                "Created wardrobe_wear_stats table".into()
            ),
            (
                SEED_MIGRATION_ID.into(),
                4,
                "Credited 4 wear(s) across 2 chat(s)".into()
            ),
        ]
    );
    assert!(lines.iter().any(|l| l
        == "INFO quilltap::boot Created the wardrobe wear ledger migrationId=add-wardrobe-wear-stats-table-v1"));
    assert!(lines.iter().any(|l| l
        == "INFO quilltap::boot Seeded the wardrobe wear ledger from current outfits migrationId=seed-wardrobe-wear-stats-v1 wears=4 chatsWithOutfits=2"));
    {
        let w = Writer::open_writable(&data.join(MAIN), PEPPER).unwrap();
        let (decl, cell): (Option<String>, Option<String>) = w
            .connection()
            .query_row(
                "SELECT (SELECT dflt_value FROM pragma_table_info('chat_settings') \
                 WHERE name = 'wardrobeImageSettings'), wardrobeImageSettings FROM chat_settings LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(decl.as_deref(), Some("'{\"imageProfileId\":null}'"));
        assert_eq!(cell.as_deref(), Some("{\"imageProfileId\":null}"));
    }

    // The second boot writes nothing.
    let before = snapshot(&data);
    let lines = boot(dir.path());
    assert_eq!(
        snapshot(&data),
        before,
        "the second boot changed the ledger"
    );
    assert!(
        !lines.iter().any(|l| l.contains("wardrobe wear ledger")),
        "the second boot logged a ledger line: {lines:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_v4_stamped_instance_is_not_seeded_again() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    derive_pre_round(&data);
    exec(
        &data,
        &format!(
            "{}; {}; {}; \
             CREATE TABLE IF NOT EXISTS \"migrations_state\" (\"id\" TEXT PRIMARY KEY, \
               \"completedAt\" TEXT NOT NULL, \"quilltapVersion\" TEXT NOT NULL, \
               \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT); \
             INSERT INTO migrations_state VALUES ('{TABLE_MIGRATION_ID}', 'x', '4.10.0', 1, 'Created wardrobe_wear_stats table'); \
             INSERT INTO migrations_state VALUES ('{SEED_MIGRATION_ID}', 'x', '4.10.0', 0, 'Credited 0 wear(s) across 0 chat(s)');",
            quilltap_core::db::wardrobe_wear_stats::WARDROBE_WEAR_STATS_DDL[0],
            quilltap_core::db::wardrobe_wear_stats::WARDROBE_WEAR_STATS_DDL[1],
            quilltap_core::db::wardrobe_wear_stats::WARDROBE_WEAR_STATS_DDL[2],
        ),
    );
    boot(dir.path());
    let (_, rows, ledger) = snapshot(&data);
    assert!(rows.is_empty(), "v4 already seeded this instance: {rows:?}");
    assert_eq!(ledger.len(), 2);
    assert_eq!(
        ledger[1].2, "Credited 0 wear(s) across 0 chat(s)",
        "v4's row untouched"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_instance_stamps_the_empty_seed_alone() {
    let _serial = SERIAL.lock().await;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    boot(dir.path());
    let (master, rows, ledger) = snapshot(&data);
    assert!(
        master
            .iter()
            .any(|(n, sql)| n == "wardrobe_wear_stats"
                && sql.contains("\"wearCount\" REAL NOT NULL")),
        "a fresh instance keeps generateDDL's table (the R-A ruling): {master:?}"
    );
    assert_eq!(master.len(), 4, "the table + three indexes: {master:?}");
    assert!(rows.is_empty());
    assert_eq!(
        ledger,
        vec![(
            SEED_MIGRATION_ID.into(),
            0,
            "Credited 0 wear(s) across 0 chat(s)".into()
        )]
    );
}
