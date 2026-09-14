//! P4.D182 — the two `31436bae4` schema moves land on ALL THREE entrances.
//!
//! `files.generationKey` + `idx_files_generationKey` (v4 `7fbf8a55b`) and
//! `chats.transcriptVersion` (v4 `5029075bb`) are re-homed from v4's migration
//! runner into two boot ensures inside `host.rs::seed_built_ins`, which
//! `assemble` runs on every boot, every unlock and first-run setup (the P4.46
//! lock-before-open order). Each entrance gets its own arm here, because
//! "assemble calls it" is a wiring claim and these are measurements.
//!
//! The SETUP arm is the sharpest of the three, and it is why both ensures
//! exist rather than just one:
//!
//! - a freshly provisioned instance gets `files.generationKey` from
//!   `provisioning/fresh_schema.json` (v4's `FileEntrySchema` declares it) but
//!   NOT the index — `generateDDL` cannot express a plain index, measured at
//!   the re-dump;
//! - and it gets `chats.transcriptVersion` from NOWHERE — v4 keeps the field
//!   out of `ChatMetadataSchema` deliberately, so `generateDDL` never emits it
//!   and the dump provably carries zero occurrences.
//!
//! So on a fresh instance the index and that column can ONLY have come from
//! the ensures.

use std::path::Path;

use quilltap_core::api::{PepperState, QuilltapCore as _, Request, Response};
use quilltap_core::db::Writer;
use quilltap_core::dbkey;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const CHAT_ID: &str = "11111111-1111-4111-8111-111111111111";
const FILE_ID: &str = "22222222-2222-4222-8222-222222222222";
const USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

fn base_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.env_pepper = None;
    config
}

/// A pre-`31436bae4` instance: a `chats` table without `transcriptVersion` and
/// a `files` table without `generationKey` or its index — a genuine long-lived
/// instance whose earlier ensures have all run.
fn make_legacy_instance(base: &Path) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    w.connection()
        .execute_batch(
            "CREATE TABLE \"chats\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \"title\" TEXT, \
               \"chatType\" TEXT, \"messageCount\" REAL, \
               \"createdAt\" TEXT, \"updatedAt\" TEXT);\
             CREATE TABLE \"files\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \"sha256\" TEXT, \
               \"originalFilename\" TEXT, \"mimeType\" TEXT, \"size\" REAL, \
               \"category\" TEXT, \"source\" TEXT, \
               \"generationPrompt\" TEXT, \"generationModel\" TEXT, \
               \"generationRevisedPrompt\" TEXT, \"description\" TEXT, \
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
    w.connection()
        .execute(
            "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, \
                                category, source, createdAt, updatedAt) \
             VALUES (?1, ?2, 'ab', 'plate.webp', 'image/webp', 12, 'IMAGE', 'GENERATED', \
                     '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [FILE_ID, USER_ID],
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

fn index_exists(conn: &rusqlite::Connection, name: &str) -> bool {
    let mut stmt = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1")
        .unwrap();
    stmt.exists([name]).unwrap()
}

/// The three things this lane must be true of a live main partition.
fn assert_healed(db: &quilltap_core::db::runtime::Db) {
    let (chats_cols, files_cols, has_index) = db
        .read_main(|conn| {
            Ok((
                column_names(conn, "chats"),
                column_names(conn, "files"),
                index_exists(conn, "idx_files_generationKey"),
            ))
        })
        .unwrap();
    assert!(
        chats_cols.iter().any(|c| c == "transcriptVersion"),
        "the transcript counter column must exist: {chats_cols:?}"
    );
    assert!(
        files_cols.iter().any(|c| c == "generationKey"),
        "the avatar cache key column must exist: {files_cols:?}"
    );
    assert!(has_index, "idx_files_generationKey must exist");
}

/// The fixture must genuinely LACK all three before a boot heals them — or
/// every assertion below could pass on an instance that never needed healing
/// (the P4.88 vacuous-fixture shape).
fn assert_legacy(base: &Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    assert!(
        !column_names(w.connection(), "chats")
            .iter()
            .any(|c| c == "transcriptVersion"),
        "the legacy fixture must not already carry the transcript counter"
    );
    assert!(
        !column_names(w.connection(), "files")
            .iter()
            .any(|c| c == "generationKey"),
        "the legacy fixture must not already carry the cache key column"
    );
    assert!(
        !index_exists(w.connection(), "idx_files_generationKey"),
        "the legacy fixture must not already carry the index"
    );
}

/// Entrance 1 — an env-pepper BOOT heals a legacy instance, and the existing
/// rows read v4's documented defaults (`0` for the counter, NULL for the key:
/// v4 backfills neither, and its own migration header says `0` is correct).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_heals_both_p4d182_schema_moves() {
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

    let version: i64 = db
        .read_main(|conn| {
            conn.query_row(
                "SELECT transcriptVersion FROM chats WHERE id = ?1",
                [CHAT_ID],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(version, 0, "an existing chat starts at zero — no backfill");

    let key: Option<String> = db
        .read_main(|conn| {
            conn.query_row(
                "SELECT generationKey FROM files WHERE id = ?1",
                [FILE_ID],
                |r| r.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(key, None, "an existing file carries no cache key");
}

/// Entrance 2 — an UNLOCK heals it. The host boots locked (no env pepper,
/// a `.dbkey` on disk), so nothing has opened the partitions; the ensures run
/// inside the assembly the `Unlock` dispatch triggers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unlock_heals_both_p4d182_schema_moves() {
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

/// Entrance 3 — first-run SETUP. The discriminating arm: a freshly
/// provisioned instance replays `fresh_schema.json`, which carries
/// `files.generationKey` and CANNOT carry either the index or
/// `chats.transcriptVersion`. So both of those can only be the ensures'
/// doing, and this arm reddens if either call is dropped from the boot chain.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn setup_provisions_what_the_dump_carries_and_ensures_the_rest() {
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

    // …and the provisioned column sits in v4's `FileEntrySchema` slot, not
    // appended: the ensure recognised what the dump had already laid down and
    // added nothing. (An appended duplicate is impossible in SQLite; a second
    // ALTER would have failed the boot instead — this asserts the position,
    // which is what proves the dump, not the ensure, placed it.)
    let files_cols = db
        .read_main(|conn| Ok(column_names(conn, "files")))
        .unwrap();
    let at = files_cols
        .iter()
        .position(|c| c == "generationKey")
        .expect("the provisioned column");
    assert_eq!(files_cols[at - 1], "generationRevisedPrompt");
    assert_eq!(files_cols[at + 1], "description");
}
