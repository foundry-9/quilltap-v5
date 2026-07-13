//! P4.6y mount-OPS differential (move / copy / link / delete file, delete /
//! move / create folder, the item-route PATCH): the `api::mount_files`
//! mutation variants vs v4's REAL handlers, over per-case FRESH copies of the
//! committed mounts fixture — response bodies (incl. the `{error, code}`
//! envelopes) AND the eight-table dumps under the shared normalization
//! (`tests/mount_common/mod.rs`).
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror; jest
//! ignores .claude/ paths):
//!   TMPO=/tmp/qt-mount-ops-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp harness/oracle/cases/mount-ops.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/mounts.json "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MOUNTS_MAIN=<v5>/crates/quilltap-web/tests/fixtures/mounts-main.db \
//!   QT_FIXTURE_MOUNTS_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/mounts-mount.db \
//!   QT_MOUNTS_FS_TREE=<v5>/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
//!   QT_ORACLE_OUT=/tmp/oracle-mount-ops.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- mount-ops
//! Run:
//!   QT_ORACLE_MOUNT_OPS=/tmp/oracle-mount-ops.ndjson \
//!     cargo test -p quilltap-harness --test mount_ops_equivalence

#[path = "mount_common/mod.rs"]
mod mount_common;

use mount_common::*;
use quilltap_core::api::mount_files as mf;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::Db;
use serde_json::json;

#[test]
fn mount_ops_match_oracle() {
    let Some(oracle) = load_oracle("QT_ORACLE_MOUNT_OPS") else {
        return;
    };
    let pepper = test_pepper();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    type CaseFn = Box<dyn Fn(&Db, &tokio::runtime::Runtime) -> Response>;

    let cases: Vec<(&str, CaseFn)> = vec![
        // ── copy ──
        (
            "copy_db_db",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "notes/copied.md",
                    None,
                ))
            }),
        ),
        (
            "copy_db_db_force",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "notes/copied.md",
                    Some(true),
                ))
            }),
        ),
        (
            "copy_dest_exists",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_DB,
                    "reference.txt",
                    None,
                ))
            }),
        ),
        (
            "copy_same_path",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_DB,
                    "notes/intro.md",
                    None,
                ))
            }),
        ),
        (
            "copy_fs_fs",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_FS,
                    "notes/alpha.md",
                    MP_FS,
                    "docs/alpha-copy.md",
                    None,
                ))
            }),
        ),
        (
            "copy_fs_db",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_FS,
                    "notes/alpha.md",
                    MP_DB,
                    "imported/alpha.md",
                    None,
                ))
            }),
        ),
        (
            "copy_db_fs",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_FS,
                    "out/intro.md",
                    None,
                ))
            }),
        ),
        (
            "copy_blob_db_db",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db,
                    MP_DB,
                    "images/logo.png",
                    MP_EMPTY,
                    "images/logo.png",
                    None,
                ))
            }),
        ),
        (
            "copy_missing_source",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db, MP_DB, "nope.md", MP_EMPTY, "x.md", None,
                ))
            }),
        ),
        // ── move ──
        (
            "move_db_db",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_move(
                    db,
                    MP_DB,
                    "reference.txt",
                    MP_EMPTY,
                    "moved/reference.txt",
                ))
            }),
        ),
        (
            "move_fs_fs",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_move(
                    db,
                    MP_FS,
                    "notes/beta.txt",
                    MP_FS,
                    "notes/beta-renamed.txt",
                ))
            }),
        ),
        (
            "move_cross_db_fs",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_move(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_FS,
                    "moved-intro.md",
                ))
            }),
        ),
        (
            "move_dest_exists",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_move(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_DB,
                    "reference.txt",
                ))
            }),
        ),
        // ── link ──
        (
            "link_db_db",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "linked/intro.md",
                ))
            }),
        ),
        (
            "link_fs_fs",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_link(
                    db,
                    MP_FS,
                    "notes/alpha.md",
                    MP_FS,
                    "linked-alpha.md",
                ))
            }),
        ),
        (
            "link_cross",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_FS,
                    "x.md",
                ))
            }),
        ),
        (
            "link_dest_exists",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_DB,
                    "reference.txt",
                ))
            }),
        ),
        // ── delete file ──
        (
            "delete_file_db",
            Box::new(|db, rt| rt.block_on(mf::mount_file_delete(db, MP_DB, "reference.txt"))),
        ),
        (
            "delete_file_fs",
            Box::new(|db, rt| rt.block_on(mf::mount_file_delete(db, MP_FS, "notes/alpha.md"))),
        ),
        (
            "delete_file_missing",
            Box::new(|db, rt| rt.block_on(mf::mount_file_delete(db, MP_DB, "nope.md"))),
        ),
        // ── folders ──
        (
            "folder_delete_db_nonempty",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_delete(db, MP_DB, "notes"))),
        ),
        (
            "folder_delete_db_missing",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_delete(db, MP_DB, "ghost"))),
        ),
        (
            "folder_delete_fs_nonempty",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_delete(db, MP_FS, "notes"))),
        ),
        (
            "folder_delete_fs_missing",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_delete(db, MP_FS, "ghost"))),
        ),
        (
            "folder_move_db",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_move(db, MP_DB, "notes", "notes2"))),
        ),
        (
            "folder_move_fs_after_scan",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(mf::mount_folder_move(db, MP_FS, "notes", "moved-notes"))
            }),
        ),
        (
            "folder_move_same",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_move(db, MP_DB, "notes", "notes"))),
        ),
        (
            "folder_move_dest_exists",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_move(db, MP_FS, "notes", "data"))),
        ),
        // ── item-route PATCH ──
        (
            "patch_rename",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_update(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    None,
                    Some("notes/intro2.md".to_string()),
                ))
            }),
        ),
        (
            "patch_desc_blob",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_update(
                    db,
                    MP_DB,
                    "images/logo.png",
                    Some("A fresh caption.".to_string()),
                    None,
                ))
            }),
        ),
        (
            "patch_desc_text",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_update(
                    db,
                    MP_DB,
                    "reference.txt",
                    Some("nope".to_string()),
                    None,
                ))
            }),
        ),
        (
            "patch_rename_and_desc",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_update(
                    db,
                    MP_DB,
                    "images/logo.png",
                    Some("Moved & described.".to_string()),
                    Some("images/badge.png".to_string()),
                ))
            }),
        ),
        (
            "patch_no_fields",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_update(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    None,
                    None,
                ))
            }),
        ),
        (
            "patch_rename_missing",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_update(
                    db,
                    MP_DB,
                    "ghost.md",
                    None,
                    Some("ghost2.md".to_string()),
                ))
            }),
        ),
        // ── folder create ──
        (
            "folder_create_db",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_DB, "fresh/sub"))),
        ),
        (
            "folder_create_fs",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_FS, "fresh/sub"))),
        ),
        (
            "folder_create_unsafe",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_DB, "../evil"))),
        ),
        (
            "folder_create_bad_chars",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_DB, "a<b"))),
        ),
        (
            "folder_create_then_delete_db",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_folder_create(db, MP_DB, "scratch"));
                rt.block_on(mf::mount_folder_delete(db, MP_DB, "scratch"))
            }),
        ),
    ];

    let mut checked = 0usize;
    for (name, run) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("no oracle row {name}"));
        let (db, scratch) = fresh_db(&pepper, name);
        let resp = run(&db, &rt);
        let (status, body) = response_to_status_body(resp);
        let tables = dump_tables(&db);
        let got = json!({ "name": name, "status": status, "body": body, "tables": tables });
        assert_eq!(
            norm(&got),
            norm(want),
            "case {name}\n{}",
            diff_first_line(&norm(&got), &norm(want))
        );
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
        checked += 1;
    }

    assert_eq!(checked, 39, "expected the 39 mount-ops cases");
    eprintln!("OK: mount-ops matched oracle ({checked} cases).");
}
