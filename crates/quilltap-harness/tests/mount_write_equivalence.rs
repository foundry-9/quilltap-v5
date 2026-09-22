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
//! P4.104 (bug 159's image half, v4 `186eb09cb`): two rows hand the write a
//! DECODABLE PNG (the `normalize-blob-image` seed) and compare the stored
//! row's D19 `imageFacts` (`blob_image_facts/mod.rs`); the encoder-owned
//! cells of that row (the file/blob sha + size, the upload body's sha + size)
//! are blanked on BOTH sides and the sha-ordered tables re-sorted — sharp and
//! the host encoder never produce the same WebP bytes. Both rows run under
//! [`blob_webp`], the host encoder production wires:
//!  - `write_raw_real_png` → `file_ops::write_dest_bytes`'s blob arm (v4
//!    `writeFile` → `linkBlobContent`): the normalization is the ONLY encoder
//!    on this path. The body stays compared whole — it answers the
//!    PRE-normalization sha and `.png` path on both sides.
//!  - `blob_upload_real_png` → `store_mount_file` (v4 `store-file.ts:250`'s
//!    pre-transcode, then `linkBlobContent`'s normalization at `:281`). The
//!    pre-transcode already yields lossy WebP, which the normalization then
//!    declines — both key on the same mime with the same encoder, so on this
//!    route (every v5 caller passes `transcode_images: true`) the
//!    normalization is redundant and a mutation of it alone survives; the row
//!    pins the pipeline's OUTPUT, not which of the two stages produced it.
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror; jest
//! ignores .claude/ paths):
//!   TMPO=/tmp/qt-mount-write-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures" "$TMPO/lib"
//!   cp harness/oracle/cases/mount-write.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/mounts.json "$TMPO/fixtures/"
//!   cp harness/oracle/lib/blob-image-facts.ts "$TMPO/lib/"
//!   cp harness/oracle/fixtures/normalize-blob-image/photo.png "$TMPO/fixtures/"
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

#[path = "blob_image_facts/mod.rs"]
mod blob_image_facts;

use base64::Engine;
use mount_common::*;
use quilltap_core::api::mount_files as mf;
use quilltap_core::api::types::Response;
use quilltap_core::db::runtime::Db;
use serde_json::{json, Value};

/// The encoder the image rows normalize through — the host's, as production
/// wires it (P4.104).
fn blob_webp(
) -> Option<std::sync::Arc<dyn quilltap_core::services::mount_index::blob_transcode::WebpTranscoder>>
{
    Some(std::sync::Arc::new(quilltap_host::HostImageCodec))
}

/// P4.104: the image rows — `(case, the stored-row filter over l.*, the
/// normalized path whose encoder-owned cells are blanked)`.
const IMAGE_ROWS: &[(&str, &str, &str)] = &[
    (
        "write_raw_real_png",
        "WHERE l.relativePath LIKE 'images/raw.%' ORDER BY l.relativePath",
        "images/raw.webp",
    ),
    (
        "blob_upload_real_png",
        "WHERE l.relativePath LIKE 'images/photo.%' ORDER BY l.relativePath",
        "images/photo.webp",
    ),
];

/// Blank the encoder-owned cells of the row stored at `rel` (the file + blob
/// sha and size, and a `{blob}` body's) and re-sort the two sha-ordered tables
/// by the blanked key, so the remap walks both sides in the same order. The
/// D19 `imageFacts` carry the comparand instead. Applied identically to both
/// sides; a side with no row at `rel` is left alone (and then differs).
fn blank_encoded(case: &mut Value, rel: &str) {
    const ENC: &str = "<encoded>";
    let Some(file_id) = case["tables"]["links"].as_array().and_then(|links| {
        links
            .iter()
            .find(|l| l["relativePath"] == rel)
            .map(|l| l["fileId"].clone())
    }) else {
        return;
    };
    let tables = &mut case["tables"];
    if let Some(files) = tables["files"].as_array_mut() {
        for f in files.iter_mut().filter(|f| f["id"] == file_id) {
            f["sha256"] = json!(ENC);
            f["fileSizeBytes"] = json!(ENC);
        }
        files.sort_by_key(|f| {
            format!(
                "{}\u{0}{}",
                f["sha256"].as_str().unwrap_or(""),
                f["source"].as_str().unwrap_or("")
            )
        });
    }
    if let Some(blobs) = tables["blobs"].as_array_mut() {
        for b in blobs.iter_mut().filter(|b| b["fileId"] == file_id) {
            b["sha256"] = json!(ENC);
            b["sizeBytes"] = json!(ENC);
            b["dataLength"] = json!(ENC);
        }
        blobs.sort_by_key(|b| b["sha256"].as_str().unwrap_or("").to_string());
    }
    if let Some(blob) = case.get_mut("body").and_then(|b| b.get_mut("blob")) {
        if blob["fileId"] == file_id {
            blob["sha256"] = json!(ENC);
            blob["sizeBytes"] = json!(ENC);
        }
    }
}

/// Bug 157's bytes (P4.D209): uploaded to two paths they dedup onto ONE file
/// row with ONE blob and TWO links — a character vault's shape for every avatar
/// it holds at both `photos/` and `images/history/`. Undecodable, so no
/// transcode can move them on either side.
const TWIN_BYTES: &[u8] = &[137, 80, 78, 71, 13, 10, 26, 10, 7, 7, 7, 7, 7];

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
                    None,
                ))
            }),
        ),
        // P4.104: a DECODABLE PNG through the byte-preserving verb — stored
        // as WebP by `write_dest_bytes`'s blob arm (see the module doc).
        (
            "write_raw_real_png",
            200,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_write_raw(
                    db,
                    MP_DB,
                    "images/raw.png",
                    &b64(&blob_image_facts::seed_image("photo.png")),
                    None,
                    blob_webp(),
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
                    None,
                ))
            }),
        ),
        // P4.104: a DECODABLE PNG through the ingest pipeline — the
        // pre-transcode AND the normalization (see the module doc).
        (
            "blob_upload_real_png",
            201,
            Box::new(|db, rt| {
                rt.block_on(mf::mount_blob_upload(
                    db,
                    MP_DB,
                    "images/photo.png",
                    None,
                    &b64(&blob_image_facts::seed_image("photo.png")),
                    Some("image/png".to_string()),
                    Some("photo.png".to_string()),
                    blob_webp(),
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
                    None,
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
                    None,
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
                    None,
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
                    None,
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
        // Bug 157's instrument: one blob at TWO paths, each taking its own
        // caption. `runCase` re-copies the fixture per case on BOTH sides, so
        // the whole scenario lives in one case; and `blob_patch` above cannot
        // serve, because a single-link blob makes the deleted `LIMIT 1`
        // fallback correct.
        (
            "blob_patch_twin_pair",
            200,
            Box::new(|db, rt| {
                rt.block_on(async {
                    mf::mount_blob_upload(
                        db,
                        MP_DB,
                        "twins/one.bin",
                        None,
                        &b64(TWIN_BYTES),
                        Some("application/octet-stream".to_string()),
                        Some("one.bin".to_string()),
                        None,
                    )
                    .await;
                    mf::mount_blob_upload(
                        db,
                        MP_DB,
                        "twins/two.bin",
                        None,
                        &b64(TWIN_BYTES),
                        Some("application/octet-stream".to_string()),
                        Some("two.bin".to_string()),
                        None,
                    )
                    .await;
                    mf::mount_blob_update(
                        db,
                        MP_DB,
                        "twins/one.bin",
                        Some("the caption belongs to this path".to_string()),
                    )
                    .await;
                    mf::mount_blob_update(
                        db,
                        MP_DB,
                        "twins/two.bin",
                        Some("and this one belongs to the other".to_string()),
                    )
                    .await
                })
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
        let mut got = json!({ "name": name, "status": status, "body": body, "tables": tables });
        let mut want = want.clone();
        let image_row = IMAGE_ROWS.iter().find(|(n, _, _)| n == name);
        if let Some((_, filter, rel)) = image_row {
            // P4.104: the D19 comparand for the decodable row.
            let input = blob_image_facts::seed_image("photo.png");
            let facts = db
                .read_mount_index(|mount| {
                    Ok(blob_image_facts::blob_image_facts(
                        &blob_image_facts::stored_blob_rows(mount, filter, &[]),
                        &input,
                    ))
                })
                .unwrap();
            let got_facts = Value::Array(facts);
            let want_facts = want.get("imageFacts").cloned().unwrap_or_else(|| {
                panic!("case {name}: the oracle carries no imageFacts — regenerate")
            });
            assert_eq!(got_facts, want_facts, "case {name}: imageFacts");
            eprintln!("[{name}] imageFacts OK: {got_facts}");
            want.as_object_mut().unwrap().remove("imageFacts");
            blank_encoded(&mut got, rel);
            blank_encoded(&mut want, rel);
        }
        let want = &want;
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

    // 22 + 1 (P4.D209 added `blob_patch_twin_pair`, bug 157's instrument)
    // + 2 (P4.104's decodable-image rows).
    assert_eq!(checked, 25, "expected the 25 mount-write cases");
    eprintln!("OK: mount-write matched oracle ({checked} cases).");
}
