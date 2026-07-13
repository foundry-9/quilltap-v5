//! P4.6y mount-WRITE differential (the storeMountFile ingest PUT, the
//! byte-preserving `?action=write-file` verb, and the blobs
//! collection/item routes): the `api::mount_files` write/blob variants vs v4's
//! REAL handlers, over per-case FRESH copies of the committed mounts fixture —
//! bodies AND the eight-table dumps under the shared normalization
//! (`tests/mount_common/mod.rs`). The post-write refresh chain runs REAL on
//! both sides (v4 drained; v5 synchronous under the single-writer model).
//!
//! The blob-upload 201s are a TRANSPORT status (v4 `created()`): the dispatch
//! envelope is 200 and the web edge's blobs POST route maps to 201 — the
//! runner pins that mapping per case.
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror; jest
//! ignores .claude/ paths):
//!   TMPO=/tmp/qt-mount-write-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp harness/oracle/cases/mount-write.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/mounts.json "$TMPO/fixtures/"
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_MOUNTS_MAIN=<v5>/crates/quilltap-web/tests/fixtures/mounts-main.db \
//!   QT_FIXTURE_MOUNTS_MOUNT=<v5>/crates/quilltap-web/tests/fixtures/mounts-mount.db \
//!   QT_MOUNTS_FS_TREE=<v5>/crates/quilltap-web/tests/fixtures/mounts-fs-tree \
//!   QT_ORACLE_OUT=/tmp/oracle-mount-write.ndjson \
//!     npx jest --silent --watchman=false --testTimeout=300000 \
//!       --roots "$PWD" --roots "$TMPO/cases" -- mount-write
//! Run:
//!   QT_ORACLE_MOUNT_WRITE=/tmp/oracle-mount-write.ndjson \
//!     cargo test -p quilltap-harness --test mount_write_equivalence

#[path = "mount_common/mod.rs"]
mod mount_common;

use base64::Engine;
use mount_common::*;
use quilltap_core::api::mount_files as mf;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::Db;
use serde_json::json;

const GARBAGE_PNG: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 9, 9, 9, 9];
const FAKE_WEBP: &[u8] = b"RIFF0000WEBPVP8 fake-but-webp-typed";
const GARBAGE_PDF: &[u8] = b"definitely not a pdf either";

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// intro.md's stored lastModified as epoch ms (the expected-mtime happy path
/// reads it from THIS side's fixture copy, exactly like the oracle does).
fn intro_mtime_ms(db: &Db) -> i64 {
    let iso: String = db
        .read_mount_index(|conn| {
            conn.query_row(
                "SELECT lastModified FROM doc_mount_file_links \
                 WHERE mountPointId = ?1 AND relativePath = ?2",
                rusqlite::params![MP_DB, "notes/intro.md"],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .expect("intro lastModified");
    quilltap_core::clock::iso_to_ms(&iso).unwrap_or(0)
}

#[test]
fn mount_write_matches_oracle() {
    let Some(oracle) = load_oracle("QT_ORACLE_MOUNT_WRITE") else {
        return;
    };
    let pepper = test_pepper();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();

    type CaseFn = Box<dyn Fn(&Db, &tokio::runtime::Runtime) -> Response>;
    // (name, expected-success-status-at-the-edge, runner)
    let cases: Vec<(&str, i64, CaseFn)> = vec![
        (
            "put_json_new_md",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_DB,
                    "notes/fresh.md",
                    "# Fresh\n\nA brand new body line.\n",
                    None,
                    None,
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_json_overwrite",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    "# Intro v2\n\nRewritten body.\n",
                    None,
                    None,
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_json_conflict",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    "x",
                    None,
                    Some(12345),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_json_expected_ok",
            200,
            Box::new(|db, rt| {
                let mtime = intro_mtime_ms(db);
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    "# Intro v3\n\nGuarded rewrite.\n",
                    None,
                    Some(mtime),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_fs_new",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_FS,
                    "notes/created.txt",
                    "created on disk\nsecond line\n",
                    None,
                    None,
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_fs_conflict",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_FS,
                    "notes/alpha.md",
                    "x",
                    None,
                    Some(12345),
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_json_blob_png",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_DB,
                    "images/new.png",
                    &b64(GARBAGE_PNG),
                    Some("base64"),
                    None,
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "put_pdf",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write(
                    db,
                    MP_DB,
                    "docs/up.pdf",
                    &b64(GARBAGE_PDF),
                    Some("base64"),
                    None,
                    None,
                    None,
                    None,
                ))
            }),
        ),
        (
            "write_raw_action",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write_raw(
                    db,
                    MP_DB,
                    "raw/out.md",
                    &b64(b"# Raw\n\nbyte-preserving write\n"),
                    None,
                ))
            }),
        ),
        (
            "write_raw_exists",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write_raw(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    &b64(b"overwrite attempt"),
                    None,
                ))
            }),
        ),
        (
            "blob_upload_png",
            201,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_upload(
                    db,
                    MP_DB,
                    "images/upload.png",
                    Some("A caption.".to_string()),
                    &b64(GARBAGE_PNG),
                    Some("image/png".to_string()),
                    Some("upload.png".to_string()),
                ))
            }),
        ),
        (
            "blob_upload_webp",
            201,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_upload(
                    db,
                    MP_DB,
                    "images/asis.webp",
                    None,
                    &b64(FAKE_WEBP),
                    Some("image/webp".to_string()),
                    Some("asis.webp".to_string()),
                ))
            }),
        ),
        (
            "blob_upload_md",
            201,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_upload(
                    db,
                    MP_DB,
                    "notes/uploaded.md",
                    None,
                    &b64(b"# Uploaded\n\nvia blobs route\n"),
                    Some("text/markdown".to_string()),
                    Some("uploaded.md".to_string()),
                ))
            }),
        ),
        (
            "blob_upload_pdf",
            201,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_upload(
                    db,
                    MP_DB,
                    "docs/uploaded.pdf",
                    None,
                    &b64(GARBAGE_PDF),
                    Some("application/pdf".to_string()),
                    Some("uploaded.pdf".to_string()),
                ))
            }),
        ),
        (
            "blob_upload_empty",
            201,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_upload(
                    db,
                    MP_DB,
                    "images/empty.png",
                    None,
                    "",
                    Some("image/png".to_string()),
                    Some("empty.png".to_string()),
                ))
            }),
        ),
        (
            "blobs_list",
            200,
            Box::new(|db, _| mf::mount_blobs_list(db, MP_DB, None)),
        ),
        (
            "blobs_list_folder",
            200,
            Box::new(|db, _| mf::mount_blobs_list(db, MP_DB, Some("images"))),
        ),
        (
            "blob_delete",
            200,
            Box::new(|db, rt| rt.block_on(mf::mount_blob_delete(db, MP_DB, "images/logo.png"))),
        ),
        (
            "blob_delete_doc_fallback",
            200,
            Box::new(|db, rt| rt.block_on(mf::mount_blob_delete(db, MP_DB, "notes/intro.md"))),
        ),
        (
            "blob_delete_missing",
            200,
            Box::new(|db, rt| rt.block_on(mf::mount_blob_delete(db, MP_DB, "nope.bin"))),
        ),
        (
            "blob_patch",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_update(
                    db,
                    MP_DB,
                    "images/logo.png",
                    Some("Recaptioned.".to_string()),
                ))
            }),
        ),
        (
            "blob_patch_missing",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_update(
                    db,
                    MP_DB,
                    "nope.bin",
                    Some("x".to_string()),
                ))
            }),
        ),
    ];

    let mut checked = 0usize;
    for (name, created_status, run) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("no oracle row {name}"));
        let (db, scratch) = fresh_db(&pepper, name);
        let resp = run(&db, &rt);
        let (mut status, body) = response_to_status_body(resp);
        // The web edge maps the blob-upload success envelope to v4's 201.
        if status == 200 && *created_status == 201 {
            status = 201;
        }
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

    assert_eq!(checked, 22, "expected the 22 mount-write cases");
    eprintln!("OK: mount-write matched oracle ({checked} cases).");
}
