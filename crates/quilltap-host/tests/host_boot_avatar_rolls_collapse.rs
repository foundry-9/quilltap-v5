//! P4.D184 — the collapse-duplicate-avatar-rolls pass runs on BOOT, and the
//! ledger decides whether it runs at all.
//!
//! `db::avatar_rolls_collapse_heal` is proven against v4's real migration by
//! `avatar_rolls_collapse_heal_equivalence`. What that family cannot say is
//! whether anything CALLS it: the wire lives in `host.rs::seed_built_ins`, after
//! P4.D182's `files` ensure (the pass reads and writes `generationKey`, which on
//! a pre-4.10 instance does not exist until that call) and inside the
//! mount-aware block (it needs both partitions to take a victim's blob).
//!
//! Four arms, because each is a different claim:
//!
//!   1. a planted duplicate population IS collapsed on boot — the wire exists;
//!   2. a SECOND boot changes nothing — the ledger row the first boot wrote is
//!      honoured by this app;
//!   3. a boot on an instance whose ledger already carries v4's OWN row
//!      collapses NOTHING — the cross-app direction, and the one the real Friday
//!      instance will meet, since v4 ran this migration there before any v5 walk;
//!   4. [P4.D192, v4 `23abc1ba1`, bug 145] a victim the operator had KEPT in a
//!      character's album loses its own link and its `files` row, and keeps the
//!      album link and the bytes behind it. The tier-2 family proves that of the
//!      function; only a real boot proves it of the whole wire, over the mount
//!      index's REAL DDL rather than the migration suite's reduced one.

use std::path::Path;

use quilltap_core::api::{QuilltapCore as _, Request, Response};
use quilltap_core::db::Writer;
use quilltap_host::{Host, HostConfig};

const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
const OLD_ID: &str = "aaaaaaaa-1111-4111-8111-111111111111";
const NEW_ID: &str = "aaaaaaaa-2222-4222-8222-222222222222";
const PROMPT: &str = "Solo portrait of a single woman: Friday. Wearing a green coat.";

fn base_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.env_pepper = Some(PEPPER.to_string());
    config
}

/// Two unkeyed avatar rolls of ONE configuration, each with its vault blob —
/// the shape the pass exists to collapse. `keep_victim_in_album` adds the second
/// link over the victim's bytes that the operator would have made by keeping
/// that plate (v4 `23abc1ba1`, bug 145).
fn plant_duplicate_rolls(base: &Path, with_v4_ledger_row: bool) {
    plant(base, with_v4_ledger_row, false)
}

fn plant(base: &Path, with_v4_ledger_row: bool, keep_victim_in_album: bool) {
    let data = base.join("data");
    std::fs::create_dir_all(&data).unwrap();

    let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
    w.connection()
        .execute_batch(
            "CREATE TABLE \"files\" (\
               \"id\" TEXT PRIMARY KEY, \"userId\" TEXT, \"sha256\" TEXT, \
               \"originalFilename\" TEXT, \"mimeType\" TEXT, \"size\" REAL, \
               \"category\" TEXT, \"source\" TEXT, \
               \"generationPrompt\" TEXT, \"generationModel\" TEXT, \
               \"generationRevisedPrompt\" TEXT, \"description\" TEXT, \
               \"storageKey\" TEXT, \"tags\" TEXT, \
               \"createdAt\" TEXT, \"updatedAt\" TEXT);",
        )
        .unwrap();
    for (id, created) in [
        (OLD_ID, "2026-01-01T00:00:00.000Z"),
        (NEW_ID, "2026-06-01T00:00:00.000Z"),
    ] {
        w.connection()
            .execute(
                "INSERT INTO files (id, userId, sha256, originalFilename, mimeType, size, \
                                    category, source, generationPrompt, generationModel, \
                                    storageKey, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'ab', ?3, 'image/webp', 12, 'IMAGE', 'GENERATED', ?4, \
                         'flux-dev', ?5, ?6, ?6)",
                rusqlite::params![
                    id,
                    USER_ID,
                    format!("avatar_Friday_{id}.webp"),
                    PROMPT,
                    format!("mount-blob:mount-1:blob-{id}"),
                    created
                ],
            )
            .unwrap();
    }
    if with_v4_ledger_row {
        // v4 ran its own migration on this instance already.
        w.connection()
            .execute_batch(
                "CREATE TABLE \"migrations_state\" (\
                   \"id\" TEXT PRIMARY KEY, \"completedAt\" TEXT NOT NULL, \
                   \"quilltapVersion\" TEXT NOT NULL, \
                   \"itemsAffected\" INTEGER NOT NULL DEFAULT 0, \"message\" TEXT);\
                 CREATE TABLE \"migrations_metadata\" (\
                   \"key\" TEXT PRIMARY KEY, \"value\" TEXT NOT NULL);\
                 INSERT INTO migrations_state \
                   (id, completedAt, quilltapVersion, itemsAffected, message) \
                 VALUES ('collapse-duplicate-avatar-rolls-v1', \
                         '2026-09-13T00:00:00.000Z', '4.10.0', 42, 'v4 ran this');",
            )
            .unwrap();
    }
    drop(w);

    let m = Writer::open_writable(&data.join("quilltap-mount-index.db"), PEPPER).unwrap();
    // The five mount tables at their REAL shapes (column lists copied from
    // `provisioning/fresh_schema.json`, foreign keys dropped as a hand-built
    // legacy fixture has none). A reduced DDL is not enough here: this is a
    // whole boot, and the sibling passes that run alongside the collapse read
    // columns the collapse itself never touches (`doc_mount_blobs.sha256` is
    // what the P4.D152 realign reads, and its absence failed the first run of
    // this test with "no such column: sha256").
    m.connection()
        .execute_batch(
            "CREATE TABLE \"doc_mount_files\" (\
               \"id\" TEXT PRIMARY KEY NOT NULL, \"sha256\" TEXT NOT NULL, \
               \"fileSizeBytes\" REAL NOT NULL, \"fileType\" TEXT NOT NULL, \
               \"source\" TEXT DEFAULT 'filesystem', \
               \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);\
             CREATE TABLE \"doc_mount_blobs\" (\
               \"id\" TEXT PRIMARY KEY, \"fileId\" TEXT NOT NULL, \
               \"sha256\" TEXT NOT NULL, \"sizeBytes\" INTEGER NOT NULL, \
               \"storedMimeType\" TEXT NOT NULL, \"data\" BLOB NOT NULL, \
               \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);\
             CREATE TABLE \"doc_mount_documents\" (\
               \"id\" TEXT PRIMARY KEY NOT NULL, \"fileId\" TEXT NOT NULL, \
               \"content\" TEXT NOT NULL, \"contentSha256\" TEXT NOT NULL, \
               \"plainTextLength\" REAL NOT NULL, \
               \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);\
             CREATE TABLE \"doc_mount_file_links\" (\
               \"id\" TEXT PRIMARY KEY NOT NULL, \"fileId\" TEXT NOT NULL, \
               \"linkGroupId\" TEXT, \"mountPointId\" TEXT NOT NULL, \
               \"relativePath\" TEXT NOT NULL, \"fileName\" TEXT NOT NULL, \
               \"folderId\" TEXT, \"chunkCount\" REAL DEFAULT 0, \
               \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);\
             CREATE TABLE \"doc_mount_chunks\" (\
               \"id\" TEXT PRIMARY KEY NOT NULL, \"linkId\" TEXT NOT NULL, \
               \"mountPointId\" TEXT NOT NULL, \"chunkIndex\" REAL NOT NULL, \
               \"content\" TEXT NOT NULL, \"tokenCount\" REAL NOT NULL, \
               \"createdAt\" TEXT NOT NULL, \"updatedAt\" TEXT NOT NULL);",
        )
        .unwrap();
    for id in [OLD_ID, NEW_ID] {
        m.connection()
            .execute(
                "INSERT INTO doc_mount_files \
                   (id, sha256, fileSizeBytes, fileType, createdAt, updatedAt) \
                 VALUES (?1, 'ab', 12, 'image', '2026-01-01T00:00:00.000Z', \
                         '2026-01-01T00:00:00.000Z')",
                [format!("content-{id}")],
            )
            .unwrap();
        m.connection()
            .execute(
                "INSERT INTO doc_mount_blobs \
                   (id, fileId, sha256, sizeBytes, storedMimeType, data, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'ab', 12, 'image/webp', X'00', \
                         '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
                rusqlite::params![format!("blob-{id}"), format!("content-{id}")],
            )
            .unwrap();
        // The roll's OWN link. Before bug 145 the pass never needed one — it
        // deleted by `fileId` — which is exactly why no arm here had modelled a
        // second link over the same bytes.
        m.connection()
            .execute(
                "INSERT INTO doc_mount_file_links \
                   (id, fileId, mountPointId, relativePath, fileName, createdAt, updatedAt) \
                 VALUES (?1, ?2, 'mount-1', ?3, ?4, '2026-01-01T00:00:00.000Z', \
                         '2026-01-01T00:00:00.000Z')",
                rusqlite::params![
                    format!("link-{id}"),
                    format!("content-{id}"),
                    format!("images/history/avatar_Friday_{id}.webp"),
                    format!("avatar_Friday_{id}.webp")
                ],
            )
            .unwrap();
    }
    if keep_victim_in_album {
        // The operator kept this plate: a SECOND link, in the character's own
        // vault's `photos/` folder, over the victim's SAME content row.
        m.connection()
            .execute(
                "INSERT INTO doc_mount_file_links \
                   (id, fileId, mountPointId, relativePath, fileName, createdAt, updatedAt) \
                 VALUES ('album-old', ?1, 'vault-1', 'photos/kept-old.webp', \
                         'kept-old.webp', '2026-01-01T00:00:00.000Z', \
                         '2026-01-01T00:00:00.000Z')",
                [format!("content-{OLD_ID}")],
            )
            .unwrap();
    }
}

/// The plant must genuinely need collapsing, or every assertion below could pass
/// on an instance that never had duplicates (the P4.88 vacuous-fixture shape).
fn assert_planted(base: &Path) {
    let w = Writer::open_writable(&base.join("data/quilltap.db"), PEPPER).unwrap();
    let n: i64 = w
        .connection()
        .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2, "the plant must carry two rolls of one configuration");
}

/// One COUNT(*) against the mount-index partition, reopened after the host has
/// been dropped (the pass writes through the host's own writer).
fn mount_count(base: &Path, sql: &str) -> i64 {
    let m = Writer::open_writable(&base.join("data/quilltap-mount-index.db"), PEPPER).unwrap();
    m.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

fn file_ids(db: &quilltap_core::db::runtime::Db) -> Vec<String> {
    db.read_main(|conn| {
        let mut stmt = conn.prepare("SELECT id FROM files ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .unwrap()
}

fn keys(db: &quilltap_core::db::runtime::Db) -> Vec<Option<String>> {
    db.read_main(|conn| {
        let mut stmt = conn.prepare("SELECT generationKey FROM files ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, Option<String>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .unwrap()
}

fn ledger_message(db: &quilltap_core::db::runtime::Db) -> Option<String> {
    db.read_main(|conn| {
        Ok(conn
            .query_row(
                "SELECT message FROM migrations_state WHERE id = ?1",
                ["collapse-duplicate-avatar-rolls-v1"],
                |r| r.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten())
    })
    .unwrap()
}

async fn boot(config: HostConfig) -> Host {
    let host = Host::start(config).unwrap();
    match host.core().dispatch(Request::Health).await {
        Response::Health(h) => assert!(h.ready),
        other => panic!("unexpected: {other:?}"),
    }
    host
}

/// Arm 1 + 2 — a boot collapses the plant, and a SECOND boot changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_collapses_duplicate_rolls_once() {
    let dir = tempfile::tempdir().unwrap();
    plant_duplicate_rolls(dir.path(), false);
    assert_planted(dir.path());

    let host = boot(base_config(dir.path())).await;
    let db = host.core().db().unwrap();
    assert_eq!(
        file_ids(&db),
        vec![NEW_ID.to_string()],
        "the newest roll survives and the older one is gone"
    );
    assert!(
        keys(&db)[0].is_some(),
        "the survivor is keyed under its v0 key"
    );
    let first_message = ledger_message(&db);
    assert!(
        first_message.is_some(),
        "the pass writes v4's ledger row when it ran"
    );
    drop(db);
    drop(host);

    // Arm 2: the ledger row this app wrote is honoured by this app.
    let host = boot(base_config(dir.path())).await;
    let db = host.core().db().unwrap();
    assert_eq!(file_ids(&db), vec![NEW_ID.to_string()]);
    assert_eq!(
        ledger_message(&db),
        first_message,
        "a second boot must not run the pass again"
    );
}

/// Arm 4 — the collapse takes the roll, not the photo the operator kept.
///
/// Before bug 145's fix this deleted by `fileId`: `album-old` and `blob-old`
/// both went, silently, and nothing outside the mount index recorded that they
/// had ever existed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn boot_keeps_an_album_copy_of_a_collapsed_roll() {
    let dir = tempfile::tempdir().unwrap();
    plant(dir.path(), false, true);
    assert_planted(dir.path());
    // The plant must genuinely carry TWO links over the victim's bytes, or the
    // arm proves nothing it did not already prove in arm 1.
    assert_eq!(
        mount_count(
            dir.path(),
            &format!("SELECT COUNT(*) FROM doc_mount_file_links WHERE fileId = 'content-{OLD_ID}'")
        ),
        2,
        "the victim must start with its own link AND the album copy"
    );

    let host = boot(base_config(dir.path())).await;
    let db = host.core().db().unwrap();
    assert_eq!(
        file_ids(&db),
        vec![NEW_ID.to_string()],
        "the duplicate cache row still goes — this is about the bytes"
    );
    drop(db);
    drop(host);

    // The roll's own link is gone; the album's link and its bytes are not.
    assert_eq!(
        mount_count(
            dir.path(),
            &format!("SELECT COUNT(*) FROM doc_mount_file_links WHERE id = 'link-{OLD_ID}'")
        ),
        0,
        "the roll's own link goes"
    );
    assert_eq!(
        mount_count(
            dir.path(),
            "SELECT COUNT(*) FROM doc_mount_file_links WHERE id = 'album-old'"
        ),
        1,
        "the photo the operator kept stays"
    );
    assert_eq!(
        mount_count(
            dir.path(),
            &format!("SELECT COUNT(*) FROM doc_mount_blobs WHERE id = 'blob-{OLD_ID}'")
        ),
        1,
        "…and so do the bytes behind it"
    );
    assert_eq!(
        mount_count(
            dir.path(),
            &format!("SELECT COUNT(*) FROM doc_mount_files WHERE id = 'content-{OLD_ID}'")
        ),
        1,
        "…and the content row the album link still needs"
    );
}

/// Arm 3 — v4's OWN ledger row stops the pass dead. This is the arm the real
/// Friday instance meets: v4 collapsed it before any v5 boot, so v5 must find
/// the row, collapse nothing, and leave the row exactly as v4 wrote it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ledger_row_from_v4_stops_the_pass() {
    let dir = tempfile::tempdir().unwrap();
    plant_duplicate_rolls(dir.path(), true);
    assert_planted(dir.path());

    let host = boot(base_config(dir.path())).await;
    let db = host.core().db().unwrap();

    assert_eq!(
        file_ids(&db),
        vec![OLD_ID.to_string(), NEW_ID.to_string()],
        "v4 already ran this pass — v5 must collapse nothing"
    );
    assert_eq!(
        keys(&db),
        vec![None, None],
        "and must key nothing either: the whole pass is skipped, not just the delete"
    );
    assert_eq!(
        ledger_message(&db).as_deref(),
        Some("v4 ran this"),
        "v4's own ledger row is left exactly as v4 wrote it"
    );
}
