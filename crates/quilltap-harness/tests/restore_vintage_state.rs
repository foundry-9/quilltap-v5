//! P4.28 — restore into a **migration-vintage** instance (dogfood findings
//! #57 / #58).
//!
//! ## The blind spot this closes
//!
//! Every other restore/backup/delete differential provisions its target from
//! `fresh_schema.json`, i.e. from v4's `generateDDL`. **No real long-lived
//! instance has that schema.** Its tables were authored by `migrations/scripts/*`
//! and grown by `ALTER TABLE ADD COLUMN`, and the two shapes disagree about
//! CONSTRAINTS, which is precisely the class of thing a row-count or row-value
//! diff over a fresh target can never see:
//!
//! - `conversation_annotations` gains `UNIQUE(chatId, messageIndex,
//!   characterName)`;
//! - `doc_mount_file_links` gains `fileId` → `doc_mount_files` and
//!   `mountPointId` → `doc_mount_points`, both `ON DELETE CASCADE`, plus a
//!   UNIQUE `(mountPointId, relativePath)` index.
//!
//! v5 opens every writable connection with `PRAGMA foreign_keys = ON`, so on a
//! migrated instance those are live. That is why the 2026-08-03 Part F walk saw
//! failures the whole suite was silent about. The committed
//! `migration-vintage/` fixture is v4's own migration chain replayed from
//! nothing (see `harness/oracle/fixtures/build-migration-vintage-fixture.ts`);
//! its `doc_mount_file_links` DDL was diffed byte-for-byte against the real
//! dogfood instance's when it was built.
//!
//! ## No oracle, deliberately
//!
//! This is a v5-side invariant suite, like `restore_preview_writes_nothing` in
//! `system_restore_state`. It cannot be an equivalence: under the standing
//! 2026-08-03 backup/restore ruling v5 **diverges** here — v4 wipes no
//! annotations and skips no orphan, so v4 restoring these archives into a
//! migrated instance fails in exactly the ways the walk recorded. What v4 does
//! is asserted where it belongs (`system_delete_data_equivalence`'s
//! `ANNOTATION_DIVERGENCE_KEY`); what v5 must do instead is asserted here.
//!
//! Runs unconditionally — no env var, so it can never silently skip.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::backup::restore::{restore, RestoreMode};
use quilltap_core::services::backup::{BackupHost, HostDirs};
use quilltap_core::services::file_storage::{PixelCodec, StorageBackend};
use quilltap_core::services::provisioning::SINGLE_USER_ID;
use rusqlite::Connection;

/// The pepper `build-migration-vintage-fixture.ts` keys its instance with —
/// the same one the whole restore family uses.
const TEST_PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// A private copy of the committed migration-vintage instance.
fn vintage_instance(tag: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("qt-vintage-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let instance = root.join("instance");
    std::fs::create_dir_all(&instance).unwrap();
    let src = fixtures_dir().join("migration-vintage");
    for name in [
        "quilltap.db",
        "quilltap-mount-index.db",
        "quilltap-llm-logs.db",
    ] {
        std::fs::copy(src.join(name), instance.join(name))
            .unwrap_or_else(|e| panic!("copy {name}: {e}"));
    }
    (root, instance)
}

fn open(instance: &Path) -> Db {
    Db::open(
        DbPaths {
            main: instance.join("quilltap.db"),
            mount_index: Some(instance.join("quilltap-mount-index.db")),
            llm_logs: Some(instance.join("quilltap-llm-logs.db")),
        },
        TEST_PEPPER,
    )
    .expect("open the vintage instance")
}

struct TestHost {
    root: PathBuf,
}

impl BackupHost for TestHost {
    fn storage(&self) -> Arc<dyn StorageBackend> {
        Arc::new(quilltap_core::services::file_storage::NotConfiguredStorageBackend)
    }
    fn pixel_codec(&self) -> Arc<dyn PixelCodec> {
        Arc::new(quilltap_host::image_codec::HostImageCodec)
    }
    fn temp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }
    fn host_dirs(&self) -> HostDirs {
        HostDirs {
            npm_plugins: None,
            themes: None,
        }
    }
    fn app_version(&self) -> String {
        "5.0.0-test".into()
    }
    fn now_ms(&self) -> i64 {
        1_760_000_000_000
    }
    fn store_backup(&self, _backup_id: &str, _zip_path: &Path) {}
    fn take_backup(&self, _backup_id: &str) -> Option<PathBuf> {
        None
    }
    fn store_upload(&self, _upload_id: &str, _zip_path: &Path) {}
    fn get_upload(&self, _upload_id: &str) -> Option<PathBuf> {
        None
    }
    fn remove_upload(&self, _upload_id: &str) {}
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

fn run_restore(tag: &str, archive: &str, mode: RestoreMode) -> (PathBuf, Vec<String>, usize) {
    let (root, instance) = vintage_instance(tag);
    let zip = fixtures_dir().join("restore-archives").join(archive);
    assert!(zip.exists(), "missing committed archive: {zip:?}");
    let host = TestHost { root: root.clone() };
    std::fs::create_dir_all(host.temp_dir()).unwrap();

    let db = open(&instance);
    let summary = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(restore(&db, &host, &zip, mode, SINGLE_USER_ID))
        .expect("restore returned an error");
    drop(db);
    let links = summary.doc_mount_file_links;
    (instance, summary.warnings, links)
}

/// Every warning a constraint produced, i.e. the raw SQLite sentence leaking
/// through. A restore is allowed to warn — it is NOT allowed to warn like this,
/// because the message names no row the operator can act on.
fn constraint_warnings(warnings: &[String]) -> Vec<&String> {
    warnings
        .iter()
        .filter(|w| w.contains("constraint failed"))
        .collect()
}

/// **Finding #58.** The archive carries 9 of its 28 `doc_mount_file_links` and
/// 7 `doc_mount_folders` whose mount point was deleted out from under them (see
/// `build-restore-archive-orphan-links.test.ts` — the miniature of the 43 + 118
/// measured on the real instance). Restoring it into a migration-vintage target
/// must not produce a single bare `FOREIGN KEY constraint failed`, and the rows
/// that CAN land must all land.
#[test]
fn orphaned_doc_store_rows_are_skipped_by_name_not_by_constraint() {
    let (instance, warnings, links_restored) = run_restore(
        "orphans",
        "restore-archive-orphan-links.zip",
        RestoreMode::Replace,
    );

    let raw = constraint_warnings(&warnings);
    assert!(
        raw.is_empty(),
        "restore leaked {} raw constraint failure(s) into the operator's warning list:\n  {}",
        raw.len(),
        raw.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    // The skip must be REPORTED, not silent: the operator's library is missing
    // rows the archive contained and nothing else will ever tell them. Each of
    // the orphaned vault's nine files and seven folders gets its own sentence,
    // naming what is missing rather than which constraint objected.
    let count_of = |needle: &str| -> usize {
        warnings
            .iter()
            .filter(|w| w.starts_with(needle) && w.ends_with("is not in the backup"))
            .count()
    };
    let context = || warnings.join("\n  ");
    assert_eq!(
        count_of("Skipped doc-store file link"),
        9,
        "expected one named skip per orphaned link; warnings were:\n  {}",
        context()
    );
    assert_eq!(
        count_of("Skipped doc-store folder"),
        7,
        "expected one named skip per orphaned folder; warnings were:\n  {}",
        context()
    );
    // The orphaned vault's chunks hang off links that never landed, so they go
    // with them rather than failing on their own `linkId` foreign key.
    assert!(
        count_of("Skipped doc-store chunk") > 0,
        "chunks whose link was skipped must be skipped too; warnings were:\n  {}",
        context()
    );

    let conn = Connection::open(instance.join("quilltap-mount-index.db")).unwrap();
    conn.pragma_update(
        None,
        "key",
        format!(
            "x'{}'",
            quilltap_core::dbkey::pepper_b64_to_key_hex(TEST_PEPPER).unwrap()
        ),
    )
    .unwrap();

    // Every non-orphan link landed: the archive holds 28 links and the builder
    // orphaned 9 of them, so 19 must survive. The SUMMARY counter is the
    // measure, not the table count — restore also provisions each restored
    // character's vault and each project's/group's official store, and those
    // mint links of their own.
    assert_eq!(
        links_restored, 19,
        "the links whose store IS in the archive must all restore"
    );
    // And nothing dangles afterwards — the whole point of the skip.
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM doc_mount_file_links l \
             LEFT JOIN doc_mount_points p ON p.id = l.mountPointId WHERE p.id IS NULL"
        ),
        0,
        "a restored instance must not carry orphaned links"
    );
}

/// **Finding #57.** A `replace`-mode restore wipes first, and the wipe must
/// reach `conversation_annotations` — on this target the migrated
/// `UNIQUE(chatId, messageIndex, characterName)` is live, so a survivor makes
/// the re-insert fail. The archive carries three annotation rows.
#[test]
fn annotations_restore_onto_a_vintage_instance_without_colliding() {
    // Seed the target with the exact collision: the archive's own annotations,
    // already present. This is the state a `replace` restore arrives at on any
    // instance that has been used, and the state v4 leaves permanently.
    let (root, instance) = vintage_instance("annotations");
    {
        let conn = Connection::open(instance.join("quilltap.db")).unwrap();
        conn.pragma_update(
            None,
            "key",
            format!(
                "x'{}'",
                quilltap_core::dbkey::pepper_b64_to_key_hex(TEST_PEPPER).unwrap()
            ),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO conversation_annotations \
             (id, chatId, messageIndex, sourceMessageId, characterName, content, createdAt, updatedAt) \
             VALUES ('survivor', 'c1000000-0000-4000-8000-000000000001', 1, NULL, 'Lorian', \
                     'a note the wipe must take', '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
            [],
        )
        .expect("seed the surviving annotation");
    }
    drop(root);

    let zip = fixtures_dir()
        .join("restore-archives")
        .join("restore-archive.zip");
    let host = TestHost {
        root: instance.parent().unwrap().to_path_buf(),
    };
    std::fs::create_dir_all(host.temp_dir()).unwrap();
    let db = open(&instance);
    let summary = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(restore(
            &db,
            &host,
            &zip,
            RestoreMode::Replace,
            SINGLE_USER_ID,
        ))
        .expect("restore returned an error");
    drop(db);

    let raw = constraint_warnings(&summary.warnings);
    assert!(
        raw.is_empty(),
        "the annotation wipe did not happen — {} constraint failure(s):\n  {}",
        raw.len(),
        raw.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    let conn = Connection::open(instance.join("quilltap.db")).unwrap();
    conn.pragma_update(
        None,
        "key",
        format!(
            "x'{}'",
            quilltap_core::dbkey::pepper_b64_to_key_hex(TEST_PEPPER).unwrap()
        ),
    )
    .unwrap();
    assert_eq!(
        count(
            &conn,
            "SELECT COUNT(*) FROM conversation_annotations WHERE id = 'survivor'"
        ),
        0,
        "the pre-existing annotation must be wiped, not preserved"
    );
}
