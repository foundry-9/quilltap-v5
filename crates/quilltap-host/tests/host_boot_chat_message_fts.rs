//! P4.D204: the boot reconciler gives every instance the message search index.
//!
//! This is the test that proves the port's one structural difference from v4 is
//! handled. v4 creates the five FTS objects in the migration
//! `create-chat-message-fts-v1`; v5 has no migration runner, and its D23 schema
//! dump CANNOT carry them — the dumper keeps only `table` and `index` rows from
//! v4's `generateDDL`, so triggers and virtual tables are filtered out by
//! construction and `services/provisioning/fresh_schema.json` holds
//! 0 TRIGGER / 0 VIRTUAL / 0 fts. A freshly provisioned v5 instance therefore
//! arrives with NO index at all, and `seed_built_ins`'s reconciler is the only
//! thing that can supply it.
//!
//! Two beats:
//!
//!  1. A REAL fresh-provisioned instance, booted through `Host::start`, has all
//!     five objects — and the two counts the reconciler compares agree at zero.
//!  2. The damage v4's own module doc names — a table rebuild of
//!     `chat_messages` silently dropping the triggers with the old table — is
//!     put in by hand, and the NEXT boot restores it. Raised nothing, healed
//!     silently: exactly the contract.

use std::path::Path;

use quilltap_core::api::QuilltapCore as _;
use quilltap_core::api::{Request, Response};
use quilltap_core::db::chat_message_fts::chat_message_fts_object_names;
use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_host::{Host, HostConfig};

/// The test pepper `provision_fresh_instance`'s own self-test uses.
const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn hermetic_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    config
}

/// The names and types SQLite holds for this module's objects, sorted.
fn fts_objects_on(conn: &rusqlite::Connection) -> Vec<(String, String)> {
    let names = chat_message_fts_object_names();
    let placeholders = (0..names.len()).map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT name, type FROM sqlite_master WHERE name IN ({placeholders}) ORDER BY name"
    );
    let mut stmt = conn.prepare(&sql).expect("prepare sqlite_master");
    stmt.query_map(rusqlite::params_from_iter(names.iter()), |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })
    .expect("query sqlite_master")
    .collect::<Result<Vec<_>, _>>()
    .expect("collect sqlite_master")
}

fn fts_objects(db: &quilltap_core::db::runtime::Db) -> Vec<(String, String)> {
    db.read_main(|conn| Ok(fts_objects_on(conn)))
        .expect("read sqlite_master")
}

fn counts(db: &quilltap_core::db::runtime::Db) -> (i64, i64) {
    db.read_main(|conn| {
        Ok((
            quilltap_core::db::chat_message_fts::count_eligible_chat_messages(conn)?,
            quilltap_core::db::chat_message_fts::count_indexed_chat_messages(conn)?,
        ))
    })
    .expect("counts")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_fresh_provisioned_instance_gets_all_five_fts_objects_at_boot() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");

    // Provisioning alone cannot have created them — that is the premise of the
    // whole unit, so it is asserted rather than assumed.
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER)
            .expect("open the provisioned main db");
        assert_eq!(
            fts_objects_on(w.connection()),
            Vec::<(String, String)>::new(),
            "provisioning must NOT create the FTS objects — the D23 dumper filters \
             triggers and virtual tables out by construction; if this fires, either \
             fresh_schema.json was re-dumped by hand or the dumper changed"
        );
    }

    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready),
        other => panic!("unexpected: {other:?}"),
    }
    let db = core.db().expect("engine ready");

    assert_eq!(
        fts_objects(&db),
        vec![
            ("chat_messages_fts".to_string(), "table".to_string()),
            ("chat_messages_fts_ad".to_string(), "trigger".to_string()),
            ("chat_messages_fts_ai".to_string(), "trigger".to_string()),
            ("chat_messages_fts_au".to_string(), "trigger".to_string()),
            ("chat_messages_fts_map".to_string(), "table".to_string()),
        ],
        "the boot reconciler must supply all five objects on a fresh instance"
    );
    assert_eq!(
        counts(&db),
        (0, 0),
        "a fresh instance has no messages, so the two counts agree at zero"
    );
    drop(db);
    drop(host);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_boot_restores_triggers_a_table_rebuild_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");

    // First boot: the objects arrive.
    {
        let host = Host::start(hermetic_config(dir.path())).unwrap();
        let core = host.core();
        match core.dispatch(Request::Health).await {
            Response::Health(h) => assert!(h.ready),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(fts_objects(&core.db().expect("engine ready")).len(), 5);
        drop(host);
    }

    // The damage v4's module doc names: a table rebuild of `chat_messages`
    // takes the triggers with it and raises nothing.
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).expect("open writable");
        for name in ["ai", "ad", "au"] {
            w.connection()
                .execute_batch(&format!(r#"DROP TRIGGER "chat_messages_fts_{name}""#))
                .expect("drop the trigger");
        }
        let left = fts_objects_on(w.connection());
        assert_eq!(
            left.len(),
            2,
            "the two tables survive a trigger drop; got {left:?}"
        );
    }

    // Second boot: healed.
    let host = Host::start(hermetic_config(dir.path())).unwrap();
    let core = host.core();
    match core.dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready),
        other => panic!("unexpected: {other:?}"),
    }
    let db = core.db().expect("engine ready");
    assert_eq!(
        fts_objects(&db).len(),
        5,
        "the boot reconciler must put the dropped triggers back"
    );
    drop(db);
    drop(host);
}
