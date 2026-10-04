//! P4.D249 — `chat_informs.permanent` (v4 `52d6e7ecd`, migration
//! `add-chat-informs-permanent-v1`) lands on an existing instance at boot, and
//! a fresh one carries it in generateDDL's shape.
//!
//! The legacy arm is the discriminating one: every inform read names
//! `permanent`, so without the boot ensure the first pending-chip read on a
//! pre-`52d6e7ecd` instance fails. The setup arm proves the two v4 shapes stay
//! apart — the column sits in SCHEMA order (generateDDL's, nullable) on a fresh
//! instance and APPENDED (`NOT NULL`, the migration's) on a healed one.

use std::path::Path;

use quilltap_core::api::{PepperState, QuilltapCore as _, Request, Response};
use quilltap_core::db::chat_informs::{ChatInformCreate, ChatInformsRepository};
use quilltap_core::db::Writer;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

fn base_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.env_pepper = None;
    config
}

/// A pre-`52d6e7ecd` instance: `chat_informs` in the D23 re-dump #3 shape,
/// holding one delivered one-shot row.
fn make_legacy_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    w.connection()
        .execute_batch(
            r#"CREATE TABLE "chat_informs" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "chatId" TEXT NOT NULL,
  "batchId" TEXT NOT NULL,
  "participantId" TEXT NOT NULL,
  "contentMarkdown" TEXT NOT NULL,
  "recordMessageId" TEXT,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL,
  "consumedAt" TEXT,
  "consumedByMessageId" TEXT
);
INSERT INTO chat_informs (id, chatId, batchId, participantId, contentMarkdown,
  createdAt, updatedAt, consumedAt, consumedByMessageId)
VALUES ('old', 'c1', 'b-old', 'p1', 'An old one-shot.',
  '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z',
  '2026-01-01T00:01:00.000Z', 'm-old');"#,
        )
        .unwrap();
    drop(w);
    let _ = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
}

/// `(name, type, notnull, dflt_value)` for every `chat_informs` column.
fn table_info(conn: &rusqlite::Connection) -> Vec<(String, String, i64, Option<String>)> {
    let mut stmt = conn.prepare("PRAGMA table_info(\"chat_informs\")").unwrap();
    stmt.query_map([], |r| Ok((r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

fn table_sql(conn: &rusqlite::Connection) -> String {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE name = 'chat_informs'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

async fn boot(base: &Path) -> Host {
    let mut config = base_config(base);
    config.env_pepper = Some(PEPPER.to_string());
    let host = Host::start(config).unwrap();
    match host.core().dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready),
        other => panic!("unexpected: {other:?}"),
    }
    host
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_adds_the_column_and_a_second_boot_is_a_no_op() {
    let dir = tempfile::tempdir().unwrap();
    make_legacy_instance(dir.path());
    {
        let w = Writer::open_writable(&dir.path().join("data/quilltap.db"), PEPPER).unwrap();
        assert!(
            !table_info(w.connection())
                .iter()
                .any(|c| c.0 == "permanent"),
            "the legacy fixture must not carry the column (the P4.88 vacuous-fixture shape)"
        );
    }

    let host = boot(dir.path()).await;
    let db = host.core().db().unwrap();
    let (info, sql_after_first) = db
        .read_main(|conn| Ok((table_info(conn), table_sql(conn))))
        .unwrap();
    assert_eq!(
        info.last().unwrap(),
        &(
            "permanent".to_string(),
            "INTEGER".to_string(),
            1,
            Some("0".to_string())
        ),
        "v4's migration DDL, appended: {info:?}"
    );

    // The existing row reads as the one-shot it always was, and a delivered
    // one-shot is not in force.
    let pending = db
        .read_main(|conn| ChatInformsRepository::new(conn).find_pending_for_participant("c1", "p1"))
        .unwrap();
    assert!(pending.is_empty(), "{pending:?}");

    // A standing row round-trips through the healed table, and stays in force
    // once delivered.
    db.write(|w| {
        let repo = ChatInformsRepository::new(w.main().connection());
        repo.create(&ChatInformCreate {
            id: "standing".into(),
            chat_id: "c1".into(),
            batch_id: "b-standing".into(),
            participant_id: "p1".into(),
            content_markdown: "A standing note.".into(),
            record_message_id: None,
            permanent: true,
            created_at: "2026-01-02T00:00:00.000Z".into(),
            updated_at: "2026-01-02T00:00:00.000Z".into(),
            consumed_at: None,
            consumed_by_message_id: None,
        })?;
        repo.mark_consumed(&["standing".into()], "m-new")?;
        Ok(())
    })
    .await
    .unwrap();
    let pending = db
        .read_main(|conn| ChatInformsRepository::new(conn).find_pending_for_participant("c1", "p1"))
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].permanent);
    drop(db);
    drop(host);

    let host = boot(dir.path()).await;
    let db = host.core().db().unwrap();
    let sql_after_second = db.read_main(|conn| Ok(table_sql(conn))).unwrap();
    assert_eq!(
        sql_after_first, sql_after_second,
        "the second boot is a no-op"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_carries_the_generate_ddl_shape_in_schema_order() {
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
    let info = core
        .db()
        .unwrap()
        .read_main(|conn| Ok(table_info(conn)))
        .unwrap();
    let at = info
        .iter()
        .position(|c| c.0 == "permanent")
        .expect("the column");
    assert_eq!(info[at - 1].0, "recordMessageId");
    assert_eq!(info[at + 1].0, "createdAt");
    assert_eq!(
        (info[at].1.as_str(), info[at].2, info[at].3.as_deref()),
        ("INTEGER", 0, Some("0")),
        "generateDDL's spelling — no NOT NULL"
    );
}
