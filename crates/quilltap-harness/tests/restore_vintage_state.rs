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
//! `system_restore_state`. It asserts what v5 must do on a migration-vintage
//! restore: wipe `conversation_annotations` before re-inserting, and skip
//! orphaned rows, so a `replace`-mode restore never hits the migrated instance's
//! UNIQUE / FK constraints. v5 made both fixes first; v4 has since CONVERGED on
//! the annotation wipe (`3bb664f0`, bug 10 — now asserted as a plain equality in
//! `system_delete_data_equivalence`). The suite stays a v5-side invariant (no
//! oracle) because it pins v5's on-disk restore behavior directly, and the
//! orphan-skip half is still v5's to guarantee.
//!
//! Runs unconditionally — no env var, so it can never silently skip.
//!
//! Run:
//!   cargo test -p quilltap-harness --test restore_vintage_state

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
    let db = Db::open(
        DbPaths {
            main: instance.join("quilltap.db"),
            mount_index: Some(instance.join("quilltap-mount-index.db")),
            llm_logs: Some(instance.join("quilltap-llm-logs.db")),
        },
        TEST_PEPPER,
    )
    .expect("open the vintage instance");
    // Mirror boot. In production a restore can only run inside a booted engine,
    // and the builtin-mounts boot hook has already run v4 `40319484`'s
    // `linkGroupId` column ensure on this instance's own mount partition
    // (`services::builtin_mounts` → `ensure_link_group_column`) — the fixture
    // itself stays deliberately pre-column, because being repaired at boot IS
    // the vintage path a real instance takes. Without this, phase 22d's
    // 25-column link insert fails with "no such column: linkGroupId" on a
    // target no production restore can ever see.
    db.write_blocking(|writers| {
        quilltap_core::db::mount_index_case_repair::ensure_link_group_column(
            writers
                .mount_index()
                .expect("the vintage instance has a mount partition")
                .connection(),
        )
    })
    .expect("boot-align the vintage mount partition");
    db
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
        .block_on(restore(
            &db,
            &host,
            &zip,
            mode,
            SINGLE_USER_ID,
            Default::default(),
        ))
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

/// Every warning carrying a raw SQLite sentence — WIDER than
/// [`constraint_warnings`], and deliberately so.
///
/// This is the lane's **INSERT-tolerance survey, run as a measurement rather
/// than an inspection**. `no such column: <table>.<col>` is the failure mode of
/// dogfood #56 (`chat_settings.timezone`), and the restore orchestrator writes
/// ~38 collections through explicit column lists, any one of which could name a
/// column a migrated table lacks. Rather than adjudicate 38 call sites by
/// reading them, restore the archives into an instance whose schema v4's own
/// migration chain authored and require that **no** raw SQLite sentence reaches
/// the operator. Every phase that names a column the vintage table does not have
/// says so here, by name, in the assertion's message.
fn raw_sqlite_warnings(warnings: &[String]) -> Vec<&String> {
    warnings
        .iter()
        .filter(|w| w.contains("sqlite error"))
        .collect()
}

fn assert_no_raw_sqlite(name: &str, warnings: &[String]) {
    let raw = raw_sqlite_warnings(warnings);
    assert!(
        raw.is_empty(),
        "[{name}] {} raw SQLite sentence(s) reached the operator — a restore into a \
         migration-vintage instance must not leak them:\n  {}",
        raw.len(),
        raw.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// The INSERT-tolerance survey proper: every committed archive restored into the
/// migration-vintage instance, in BOTH modes, with the raw-SQLite gate on. An
/// archive is what drives which columns each phase actually writes, so covering
/// all of them is what makes the survey a survey rather than a spot check.
///
/// `restore-archive-malformed.zip` and `restore-archive-missing-required.zip`
/// are excluded: they are parse-failure fixtures and never reach a write.
#[test]
fn no_restore_phase_names_a_column_a_migrated_table_lacks() {
    for archive in [
        "restore-archive.zip",
        "restore-archive-legacy.zip",
        "restore-archive-minimal.zip",
        "restore-archive-uploads.zip",
        "restore-archive-gen2.zip",
        "restore-archive-memory-graph.zip",
        "restore-archive-orphan-links.zip",
    ] {
        for (suffix, mode) in [
            ("replace", RestoreMode::Replace),
            ("newacct", RestoreMode::NewAccount),
        ] {
            let tag = format!("{}-{suffix}", archive.trim_end_matches(".zip"));
            let (_instance, warnings, _links) = run_restore(&tag, archive, mode);
            assert_no_raw_sqlite(&format!("{archive} / {suffix}"), &warnings);
        }
    }
}

/// ## ⚠ KNOWN GAP — a tripwire, and the sensitivity proof for the survey above
///
/// The survey passes because the committed vintage instance has been through
/// v4's WHOLE migration chain, so its column SET matches `generateDDL`'s even
/// where its constraints do not. The instances that produced dogfood #56 are the
/// other kind: a table that never got some migration, because v5 has no
/// migration runner and v4's `generateAlterStatements` is exported and never
/// called. This test manufactures exactly that — one late-added column dropped —
/// and pins what the restore does about it today.
///
/// **It currently records a FAILURE, deliberately.** The restore's ~38 write
/// sites use explicit column lists, so a phase whose table lacks a column loses
/// every row in that collection with a raw `no such column` sentence. Making
/// them tolerant is the tier-2 repair this lane did NOT take: see the
/// adjudication table in the lane record for why (the honest fix is per-site,
/// and every site is under a byte-level differential, so it wants its own round
/// rather than a blanket sweep at the end of this one).
///
/// **When that repair lands this test FAILS**, which is the point: it is a
/// tripwire, not a permanent excuse. Delete it then — and until then it is what
/// proves `no_restore_phase_names_a_column_a_migrated_table_lacks` can actually
/// tell the two situations apart.
#[test]
fn a_column_a_migration_never_added_still_loses_that_collection() {
    let (root, instance) = vintage_instance("droppedcolumn");
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
        // `chats.timelineMode` arrives in `add-episodic-memory-fields-v1`, the
        // LAST migration in the chain — the most plausible column for a real
        // instance to be short of.
        conn.execute("ALTER TABLE chats DROP COLUMN timelineMode", [])
            .expect("drop the column");
    }

    let zip = fixtures_dir()
        .join("restore-archives")
        .join("restore-archive.zip");
    let host = TestHost { root: root.clone() };
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
            Default::default(),
        ))
        .expect("restore returned an error");
    drop(db);

    let raw = raw_sqlite_warnings(&summary.warnings);
    // SQLite says `no such column: x` on a SELECT and `table t has no column
    // named x` on an INSERT; the restore is all INSERTs, so it is the latter.
    let no_such_column: Vec<&&String> = raw
        .iter()
        .filter(|w| w.contains("no such column") || w.contains("has no column named"))
        .collect();
    assert!(
        !no_such_column.is_empty(),
        "TRIPWIRE: the restore now survives a column its table lacks — the tier-2 \
         tolerance repair has landed, so DELETE this test and keep the survey. \
         Warnings were:\n  {}",
        summary.warnings.join("\n  ")
    );
    // …and the whole chats collection is lost, which is what the repair is for.
    // Note the SUMMARY does not say so — `summary.chats` reports 2, because that
    // counter is an INPUT length for this phase rather than a write count (the
    // orchestrator's own note). So the only honest measure is the table.
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
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM chats"), 0);
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
            Default::default(),
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

// ── P4.D126: bug 103's `multiCharacterPrefill` half (v4 `e000d6bfc`) ─────────

/// The one place bug 103's headline half is observable at all.
///
/// v4's `generateDDL` declares `connection_profiles.multiCharacterPrefill` as
/// `INTEGER` with **no default**, so on every freshly-provisioned target —
/// which is what every other restore differential in this repo uses — omitting
/// the column and writing an explicit NULL land the same cell. The `DEFAULT 1`
/// lives only in the MIGRATED shape
/// (`migrations/scripts/add-profile-multi-character-prefill-field.ts:64`), i.e.
/// on exactly the instances real people restore onto.
///
/// **RED-FIRST, measured 2026-08-26 before the port**, restoring
/// `restore-archive-legacy-profiles.zip` into this fixture:
///
/// ```text
/// Both Predate      ANTHROPIC  sIU=Some(0)  mCP=Some(1)
/// Carried Both      OPENAI     sIU=Some(1)  mCP=Some(0)
/// Lowercase Legacy  openai     sIU=Some(0)  mCP=Some(1)
/// Never Capable     OLLAMA     sIU=Some(0)  mCP=Some(1)
/// Prefill Predates  ANTHROPIC  sIU=Some(1)  mCP=Some(1)
/// Stored False      GOOGLE     sIU=Some(0)  mCP=Some(1)
/// ```
///
/// Five of the six landed `multiCharacterPrefill = 1` — the `[Name]` assistant
/// prefill switched ON for profiles whose owners never chose it, Anthropic
/// included, where 4.6+ rejects an assistant tail outright and every
/// multi-character turn then 400s. Two of the six also lost their vision flag.
///
/// A v5-side invariant suite like the rest of this file: v4 is pinned on the
/// same decision by `connection_profile_legacy_fields_equivalence` (the shared
/// helper, tier-1 exact) and on the fresh-target row values by
/// `system_restore_state`'s `restore_legacy_profiles_replace`. What can only be
/// said here is what the migrated DEFAULT does about it.
#[test]
fn a_pre_49_archive_lands_never_chosen_not_the_migrated_default() {
    let (instance, warnings, _links) = run_restore(
        "legacyprofiles",
        "restore-archive-legacy-profiles.zip",
        RestoreMode::Replace,
    );
    assert_no_raw_sqlite("restore-archive-legacy-profiles.zip / replace", &warnings);

    let db = open(&instance);
    let rows: Vec<(String, Option<i64>, Option<i64>)> = db
        .read_main(|c| {
            let mut stmt = c.prepare(
                "SELECT name, supportsImageUpload, multiCharacterPrefill                  FROM connection_profiles ORDER BY name",
            )?;
            let out = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, quilltap_core::db::DbError>(out)
        })
        .expect("read the restored profiles");

    // `(name, supportsImageUpload, multiCharacterPrefill)`, in name order.
    let expected: Vec<(&str, Option<i64>, Option<i64>)> = vec![
        // ANTHROPIC, both absent → the historic map says vision, the prefill is
        // "never chosen". Pre-fix this row was `(0, 1)` — the worst of the six.
        ("Both Predate", Some(1), None),
        // both keys carried → both untouched.
        ("Carried Both", Some(1), Some(0)),
        // lowercase `openai`, both absent → the map is matched case-insensitively.
        ("Lowercase Legacy", Some(1), None),
        // OLLAMA, both absent → never capable, still never chosen.
        ("Never Capable", Some(0), None),
        // sIU carried true, prefill absent.
        ("Prefill Predates", Some(1), None),
        // GOOGLE with a STORED false and a STORED null: a truthiness seeding
        // condition would turn vision back on here. Neither key is touched.
        ("Stored False", Some(0), None),
    ];
    let got: Vec<(&str, Option<i64>, Option<i64>)> =
        rows.iter().map(|(n, i, m)| (n.as_str(), *i, *m)).collect();
    assert_eq!(
        got, expected,
        "the six seeding shapes must land what their owners chose, not the migrated table defaults"
    );
}
