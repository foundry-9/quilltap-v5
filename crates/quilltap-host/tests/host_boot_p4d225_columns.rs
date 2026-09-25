//! P4.D225 — the Concierge refusal ledger's two `chats` columns
//! (`moderationRefusalCount`, `lastModerationRefusalAt`, v4 `49059fb14`
//! migration `add-chat-refusal-ledger-v1`) land on ALL THREE entrances.
//!
//! They are re-homed from v4's migration runner into a boot ensure inside
//! `host.rs::seed_built_ins` (beside P4.D182's `transcriptVersion`), which
//! `assemble` runs on every boot, every unlock and first-run setup. Each
//! entrance gets its own arm, because "assemble calls it" is a wiring claim and
//! these are measurements.
//!
//! The SETUP arm is the discriminating one: v4 keeps both columns out of
//! `ChatMetadataSchema`, `generateDDL` walks the schema (measured at the pin:
//! the `chats` CREATE carries neither), so a freshly provisioned instance can
//! only have them from the ensure — that arm reddens if the call is dropped.

use std::path::Path;

use quilltap_core::api::{PepperState, QuilltapCore as _, Request, Response};
use quilltap_core::db::Writer;
use quilltap_core::dbkey;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const CHAT_ID: &str = "11111111-1111-4111-8111-111111111111";
const USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
const LEDGER: [&str; 2] = ["moderationRefusalCount", "lastModerationRefusalAt"];

fn base_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.env_pepper = None;
    config
}

/// A pre-`49059fb14` instance: a `chats` table without the ledger.
fn make_legacy_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    w.connection()
        .execute_batch(
            "CREATE TABLE \"chats\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \"title\" TEXT, \
               \"chatType\" TEXT, \"messageCount\" REAL, \
               \"conciergeOverride\" TEXT, \
               \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .unwrap();
    w.connection()
        .execute(
            "INSERT INTO chats (id, userId, title, chatType, messageCount, createdAt, updatedAt) \
             VALUES (?1, ?2, 'The Reading Room', 'salon', 3, \
                     '2026-01-01T00:00:00.000Z', '2026-01-02T00:00:00.000Z')",
            [CHAT_ID, USER_ID],
        )
        .unwrap();
    drop(w);
    let _ = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
}

fn column_names(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn assert_healed(db: &quilltap_core::db::runtime::Db) {
    let cols = db
        .read_main(|conn| Ok(column_names(conn, "chats")))
        .unwrap();
    for c in LEDGER {
        assert!(
            cols.iter().any(|x| x == c),
            "`chats.{c}` must exist: {cols:?}"
        );
    }
}

/// The fixture must genuinely LACK the ledger before a boot heals it (the
/// P4.88 vacuous-fixture shape).
fn assert_legacy(base: &Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    let cols = column_names(w.connection(), "chats");
    for c in LEDGER {
        assert!(
            !cols.iter().any(|x| x == c),
            "the legacy fixture must not carry `{c}`"
        );
    }
}

/// Entrance 1 — an env-pepper BOOT heals a legacy instance; the existing chat
/// reads v4's documented empty ledger (no backfill).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_heals_the_refusal_ledger_columns() {
    let dir = tempfile::tempdir().unwrap();
    make_legacy_instance(dir.path());
    assert_legacy(dir.path());

    let mut config = base_config(dir.path());
    config.env_pepper = Some(PEPPER.to_string());
    let host = Host::start(config).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready),
        other => panic!("unexpected: {other:?}"),
    }

    let db = core.db().unwrap();
    assert_healed(&db);
    let ledger = db
        .read_main(|conn| {
            Ok(quilltap_core::db::chats::ChatsRepository::new(conn)
                .get_moderation_refusal_ledger(CHAT_ID))
        })
        .unwrap();
    assert_eq!(
        ledger.count, 0,
        "an existing chat starts with an empty ledger"
    );
    assert_eq!(ledger.last_at, None);
}

/// Entrance 2 — an UNLOCK heals it (the host boots locked; the ensure runs
/// inside the assembly the `Unlock` dispatch triggers).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unlock_heals_the_refusal_ledger_columns() {
    let dir = tempfile::tempdir().unwrap();
    make_legacy_instance(dir.path());
    dbkey::save_dbkey(&dir.path().join("data"), PEPPER, "open sesame").unwrap();
    assert_legacy(dir.path());

    let host = Host::start(base_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => {
            assert!(!h.ready);
            assert_eq!(h.pepper_state, PepperState::NeedsPassphrase);
        }
        other => panic!("unexpected: {other:?}"),
    }
    assert!(core.db().is_none(), "a locked host has opened nothing");

    match core
        .dispatch(Request::Unlock {
            passphrase: "open sesame".into(),
        })
        .await
    {
        Response::UnlockState(u) => assert_eq!(u.state, PepperState::Resolved),
        other => panic!("unexpected: {other:?}"),
    }
    assert_healed(&core.db().unwrap());
}

/// Entrance 3 — first-run SETUP: the dump cannot carry the ledger, so on a
/// freshly provisioned instance the columns are the ensure's doing alone, and
/// they sit APPENDED (after the dump's last `chats` column).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_gets_the_ledger_from_the_ensure_alone() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();

    let host = Host::start(base_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => assert_eq!(h.pepper_state, PepperState::NeedsSetup),
        other => panic!("unexpected: {other:?}"),
    }
    match core
        .dispatch(Request::Setup {
            passphrase: "open sesame".into(),
        })
        .await
    {
        Response::Setup(s) => assert!(!s.requires_restart, "{s:?}"),
        other => panic!("unexpected: {other:?}"),
    }

    let db = core.db().unwrap();
    assert_healed(&db);
    let cols = db
        .read_main(|conn| Ok(column_names(conn, "chats")))
        .unwrap();
    let at = cols
        .iter()
        .position(|c| c == "moderationRefusalCount")
        .expect("the ensured column");
    assert_eq!(
        cols[at + 1],
        "lastModerationRefusalAt",
        "v4's order: count, then when"
    );
}
