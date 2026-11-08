//! P4.6y mount-OPS differential (move / copy / link / delete file, delete /
//! move / create folder, the item-route PATCH): the `api::mount_files`
//! mutation variants vs v4's REAL handlers, over per-case FRESH copies of the
//! committed mounts fixture — response bodies (incl. the `{error, code}`
//! envelopes) AND the eight-table dumps under the shared normalization
//! (`tests/mount_common/mod.rs`).
//!
//! [ed8934f1 / Bug 56] Three folder-create cases point MP_FS's basePath at
//! PLANTED unreachable conditions (missing / not-a-directory / denied) and pin
//! the new 409 diagnosis. The planted paths are fixed literals, identical on
//! both sides — that is what lets the sentences, which embed the path, be
//! compared byte-for-byte. `denied` is an unreadable PARENT, not a `chmod 000`
//! base: `stat` takes its EACCES from the parent's search permission.
//! ⚠ `containerized` is false in BOTH environments, so the container-variant
//! sentences are pinned by UNIT tests on the message builder, not here.
//!
//! Generate the oracle (Node 24, from the v4 checkout — /tmp mirror; jest
//! ignores .claude/ paths):
//!   TMPO=/tmp/qt-mount-ops-oracle
//!   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
//!   cp harness/oracle/cases/mount-ops.test.ts "$TMPO/cases/"
//!   cp harness/oracle/fixtures/mounts.json "$TMPO/fixtures/"
//!   cp harness/oracle/fixtures/docx-sample.b64 "$TMPO/fixtures/"
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

/// [ed8934f1] The planted base-path conditions — see the module header. The
/// root is per-family (the mount-points family plants `qt-bug56-mp`).
const PLANT_ROOT: &str = "/tmp/qt-bug56-ops";

/// The basePath a case wants planted on MP_FS, or `None` for the per-case
/// fs-tree copy. Mirrors the oracle case's `basePath` field.
fn planted_base_for(name: &str) -> Option<String> {
    match name {
        "folder_create_fs_base_missing" => Some(PlantedBases::missing(PLANT_ROOT)),
        "folder_create_fs_base_not_a_directory" => Some(PlantedBases::not_a_directory(PLANT_ROOT)),
        "folder_create_fs_base_denied" => Some(PlantedBases::denied(PLANT_ROOT)),
        _ => None,
    }
}

// === P4.D381 (v4 `aec626eff`): helpers for the store-browser rows ===

/// `DOCX_RENDER_HTML_REFUSED` (R-E, the human's ruling 2026-11-04): `.docx`
/// `?action=render-html` is a NAMED refusal in v5 — no mammoth twin. Each row
/// is `(case, v4 status, v4 body, v5 status, v5 body)`, asserted EXACTLY both
/// ways; every other render row (`render_html_md`, `render_html_db_md`,
/// `render_html_missing`) is a strict byte-equal row. Re-open trigger: a DOCX
/// converter ruling (`services/mount_index/docx_html.rs`).
const DOCX_RENDER_HTML_REFUSED: &[(&str, u16, &str, u16, &str)] = &[
    (
        "render_html_docx_valid",
        200,
        r#"{"html":"<p>The Scriptorium regrets nothing.</p>"}"#,
        422,
        r#"{"error":"not_available: docx render-html"}"#,
    ),
    (
        "render_html_docx_corrupt",
        500,
        r#"{"error":"Failed to render file"}"#,
        422,
        r#"{"error":"not_available: docx render-html"}"#,
    ),
];

thread_local! {
    static TREE: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// The current case's fs-tree copy.
fn tree() -> std::path::PathBuf {
    TREE.with(|t| t.borrow().clone().expect("tree set"))
}

/// The oracle's `listTree`: `dir/` / `file:<size>`, sorted by code unit.
fn list_tree(root: &std::path::Path) -> Vec<String> {
    fn walk(root: &std::path::Path, rel: &str, out: &mut Vec<String>) {
        let abs = if rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(rel)
        };
        let Ok(entries) = std::fs::read_dir(&abs) else {
            out.push(format!("{rel}/<unreadable>"));
            return;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            let meta = std::fs::symlink_metadata(root.join(&child)).unwrap();
            if meta.is_dir() {
                out.push(format!("{child}/"));
                walk(root, &child, out);
            } else {
                out.push(format!("{child}:{}", meta.len()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, "", &mut out);
    out.sort();
    out
}

fn chmod(p: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The committed DOCX plant (`harness/oracle/fixtures/docx-sample.b64`).
fn docx_sample() -> Vec<u8> {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../harness/oracle/fixtures/docx-sample.b64"),
    )
    .unwrap();
    let payload: String = text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .unwrap()
}

fn yes() -> Option<Option<serde_json::Value>> {
    Some(Some(json!(true)))
}

fn dest(d: Option<&str>) -> Option<Option<serde_json::Value>> {
    d.map(|d| Some(json!(d)))
}

async fn copy(db: &Db, mp: &str, from: &str, to: &str, d: Option<&str>) -> Response {
    mf::mount_folder_copy(
        db,
        mp,
        Some(json!(from)),
        Some(json!(to)),
        dest(d),
        SINGLE_USER_ID,
        None,
    )
    .await
}

async fn mv_to(db: &Db, mp: &str, from: &str, to: &str, d: Option<&str>) -> Response {
    mf::mount_folder_move(db, mp, from, to, dest(d), SINGLE_USER_ID, None).await
}

/// The lines the store-browser rows compare (the oracle's `P4D381_LINES`).
const P4D381_LINES: &[&str] = &[
    "Removed filesystem folder row with the folder",
    "Moved filesystem folder rows with the folder",
    "Deleted folder recursively",
    "Folder copy failed; undoing",
    "Copied folder",
    "Folder move failed; moving back",
    "Could not move a file back after a failed folder move",
    "Moved folder between stores",
    "[Mount Points v1] Deleted folder",
    "[Mount Points v1] Moved folder",
    "[Mount Points v1] Copied folder",
    "[Mount Points v1] Unlinked file from its hard-link group",
    "[Mount Points v1] Error deleting folder",
    "[Mount Points v1] Error moving folder",
    "[Mount Points v1] Error trying to copy folder",
    "[Mount Points v1] Error render file",
    "[Mount Points v1] Listed files for browser",
    "Cleared hard-link group from link",
];

/// A PROCESS-GLOBAL capture: the folder ops run inside `Db::write`, on the
/// WRITER thread. Filtered by [`P4D381_LINES`], so the sibling test's lines
/// never land in a row.
fn all_thread_lines() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    static BUF: std::sync::OnceLock<std::sync::Arc<std::sync::Mutex<Vec<String>>>> =
        std::sync::OnceLock::new();
    BUF.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let subscriber = tracing_subscriber::registry()
            .with(quilltap_core::test_support::CaptureLayer(buf.clone()));
        tracing::subscriber::set_global_default(subscriber).expect("one global capture per binary");
        buf
    })
    .clone()
}

/// `LEVEL message k=v…` with the per-side values masked: minted ids (`linkId`,
/// `groupId`) and the error text (Node's vs Rust's; v4's route ERRORs carry
/// none at all), dropped to the end of the line.
fn p4d381_mask(line: &str) -> String {
    let cut = line.find(" error=").map(|i| &line[..i]).unwrap_or(line);
    cut.split(' ')
        .map(|tok| match tok.split_once('=') {
            Some(("linkId" | "groupId", _)) => format!("{}=<id>", tok.split('=').next().unwrap()),
            _ => tok.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn p4d381_message(rest: &str) -> bool {
    P4D381_LINES
        .iter()
        .any(|m| rest == *m || rest.starts_with(&format!("{m} ")))
}

fn v5_lines(captured: &[String]) -> Vec<String> {
    captured
        .iter()
        .filter_map(|l| {
            let mut it = l.splitn(3, ' ');
            let (level, _target, rest) = (it.next()?, it.next()?, it.next()?);
            p4d381_message(rest).then(|| p4d381_mask(&format!("{level} {rest}")))
        })
        .collect()
}

/// v4's `{level, message, fields}`; a `null` field is rendered as v5's file
/// layer renders an absent value — omitted (a value-only rendering record).
fn v4_lines(row: &serde_json::Value) -> Vec<String> {
    row["logs"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|l| {
                    let mut s = format!(
                        "{} {}",
                        l["level"].as_str().unwrap().to_uppercase(),
                        l["message"].as_str().unwrap()
                    );
                    for (k, v) in l["fields"].as_object().into_iter().flatten() {
                        match v {
                            serde_json::Value::Null => {}
                            serde_json::Value::String(x) => s.push_str(&format!(" {k}={x}")),
                            other => s.push_str(&format!(" {k}={other}")),
                        }
                    }
                    p4d381_mask(&s)
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn unlink(db: &Db, mp: &str, path: Option<&str>) -> Response {
    mf::mount_file_unlink(db, mp, path.map(|p| json!(p)), SINGLE_USER_ID).await
}
// === /P4.D381 ===

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
                    None,
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
                    None,
                ))
            }),
        ),
        (
            "copy_missing_source",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_file_copy(
                    db, MP_DB, "nope.md", MP_EMPTY, "x.md", None, None,
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
                    None,
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
                    None,
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
                    None,
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
                    None,
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
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_DB,
                    "notes",
                    None,
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_db_missing",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_DB,
                    "ghost",
                    None,
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_fs_nonempty",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_FS,
                    "notes",
                    None,
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_fs_missing",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_FS,
                    "ghost",
                    None,
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_move_db",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_move(
                    db,
                    MP_DB,
                    "notes",
                    "notes2",
                    None,
                    SINGLE_USER_ID,
                    None,
                ))
            }),
        ),
        (
            "folder_move_fs_after_scan",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(mf::mount_folder_move(
                    db,
                    MP_FS,
                    "notes",
                    "moved-notes",
                    None,
                    SINGLE_USER_ID,
                    None,
                ))
            }),
        ),
        (
            "folder_move_same",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_move(
                    db,
                    MP_DB,
                    "notes",
                    "notes",
                    None,
                    SINGLE_USER_ID,
                    None,
                ))
            }),
        ),
        (
            "folder_move_dest_exists",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_move(
                    db,
                    MP_FS,
                    "notes",
                    "data",
                    None,
                    SINGLE_USER_ID,
                    None,
                ))
            }),
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
                    None,
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
                    None,
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
                    None,
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
        // [ed8934f1 / Bug 56] The store's own root is unreachable: refuse with a
        // 409 carrying the diagnosis, instead of letting the recursive mkdir
        // walk up to the topmost missing ancestor and fabricate the whole
        // chain. The basePath each of these three drives is planted by
        // `planted_base_for` below.
        (
            "folder_create_fs_base_missing",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_FS, "fresh/sub"))),
        ),
        (
            "folder_create_fs_base_not_a_directory",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_FS, "fresh/sub"))),
        ),
        (
            "folder_create_fs_base_denied",
            Box::new(|db, rt| rt.block_on(mf::mount_folder_create(db, MP_FS, "fresh/sub"))),
        ),
        (
            "folder_create_then_delete_db",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_folder_create(db, MP_DB, "scratch"));
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_DB,
                    "scratch",
                    None,
                    SINGLE_USER_ID,
                ))
            }),
        ),
        // === P4.D381 (v4 `aec626eff` / `c34986421`) — the oracle's fenced rows ===
        (
            "files_list_grouped",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "linked/intro.md",
                ));
                mf::mount_files_list(db, MP_DB)
            }),
        ),
        (
            "folder_move_fs_nested_to_top",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(mf::mount_folder_move(
                    db,
                    MP_FS,
                    "notes/sub",
                    "sub",
                    None,
                    SINGLE_USER_ID,
                    None,
                ))
            }),
        ),
        (
            "folder_delete_fs_empty_with_row",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                let _ = rt.block_on(mf::mount_file_delete(db, MP_FS, "notes/sub/deep.md"));
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_FS,
                    "notes/sub",
                    None,
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_db_recursive",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_DB,
                    "notes",
                    yes(),
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_fs_recursive",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_FS,
                    "notes/sub",
                    yes(),
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_fs_recursive_strays",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                std::fs::write(tree().join("notes/z.xyz"), "stray").unwrap();
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_FS,
                    "notes",
                    yes(),
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_fs_recursive_strays_many",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_FS,
                    "notes",
                    yes(),
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_copy_db_db",
            Box::new(|db, rt| rt.block_on(copy(db, MP_DB, "notes", "imported", Some(MP_EMPTY)))),
        ),
        (
            "folder_copy_fs_db",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(copy(db, MP_FS, "notes/sub", "fromfs", Some(MP_DB)))
            }),
        ),
        (
            "folder_copy_db_fs",
            Box::new(|db, rt| rt.block_on(copy(db, MP_DB, "notes", "fromdb", Some(MP_FS)))),
        ),
        (
            "folder_copy_into_itself",
            Box::new(|db, rt| rt.block_on(copy(db, MP_DB, "notes", "notes/inner", None))),
        ),
        (
            "folder_copy_dest_exists",
            Box::new(|db, rt| rt.block_on(copy(db, MP_DB, "notes", "docs", None))),
        ),
        (
            "folder_copy_missing",
            Box::new(|db, rt| rt.block_on(copy(db, MP_DB, "ghost", "x", None))),
        ),
        (
            "folder_move_cross_db_db",
            Box::new(|db, rt| rt.block_on(mv_to(db, MP_DB, "notes", "moved", Some(MP_EMPTY)))),
        ),
        (
            "folder_move_cross_fs_db",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(mv_to(db, MP_FS, "notes/sub", "fromfs", Some(MP_DB)))
            }),
        ),
        (
            "folder_move_cross_fs_strays",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                rt.block_on(mv_to(db, MP_FS, "notes", "x", Some(MP_DB)))
            }),
        ),
        (
            "folder_copy_undo",
            Box::new(|db, rt| {
                std::fs::write(
                    tree().join("notes/sub/deeper.md"),
                    "# Deeper\n\nFurther down.\n",
                )
                .unwrap();
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                chmod(&tree().join("notes/sub/deeper.md"), 0o000);
                let r = rt.block_on(copy(db, MP_FS, "notes/sub", "undo", Some(MP_DB)));
                chmod(&tree().join("notes/sub/deeper.md"), 0o644);
                r
            }),
        ),
        (
            "folder_move_undo",
            Box::new(|db, rt| {
                std::fs::write(
                    tree().join("notes/sub/deeper.md"),
                    "# Deeper\n\nFurther down.\n",
                )
                .unwrap();
                let _ = rt.block_on(mf::mount_scan(db, MP_FS));
                chmod(&tree().join("notes/sub/deeper.md"), 0o000);
                let r = rt.block_on(mv_to(db, MP_FS, "notes/sub", "undo", Some(MP_DB)));
                chmod(&tree().join("notes/sub/deeper.md"), 0o644);
                r
            }),
        ),
        (
            "folder_move_dest_same_store",
            Box::new(|db, rt| rt.block_on(mv_to(db, MP_DB, "notes", "notes3", Some(MP_DB)))),
        ),
        (
            "folder_delete_recursive_wrong_type",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_DB,
                    "notes",
                    Some(Some(json!("yes"))),
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_delete_recursive_null",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_delete(
                    db,
                    MP_DB,
                    "notes",
                    Some(None),
                    SINGLE_USER_ID,
                ))
            }),
        ),
        (
            "folder_copy_missing_to",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_copy(
                    db,
                    MP_DB,
                    Some(json!("notes")),
                    None,
                    None,
                    SINGLE_USER_ID,
                    None,
                ))
            }),
        ),
        (
            "folder_move_dest_empty",
            Box::new(|db, rt| {
                rt.block_on(mf::mount_folder_move(
                    db,
                    MP_DB,
                    "notes",
                    "n2",
                    Some(Some(json!(""))),
                    SINGLE_USER_ID,
                    None,
                ))
            }),
        ),
        (
            "unlink_grouped",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "linked/a.md",
                ));
                let _ = rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "linked/b.md",
                ));
                rt.block_on(unlink(db, MP_EMPTY, Some("linked/a.md")))
            }),
        ),
        (
            "unlink_dissolve",
            Box::new(|db, rt| {
                let _ = rt.block_on(mf::mount_file_link(
                    db,
                    MP_DB,
                    "notes/intro.md",
                    MP_EMPTY,
                    "linked/a.md",
                ));
                rt.block_on(unlink(db, MP_EMPTY, Some("linked/a.md")))
            }),
        ),
        (
            "unlink_ungrouped",
            Box::new(|db, rt| rt.block_on(unlink(db, MP_DB, Some("reference.txt")))),
        ),
        (
            "unlink_missing",
            Box::new(|db, rt| rt.block_on(unlink(db, MP_DB, Some("nope.md")))),
        ),
        (
            "unlink_no_path",
            Box::new(|db, rt| rt.block_on(unlink(db, MP_DB, None))),
        ),
        (
            "render_html_docx_valid",
            Box::new(|db, _rt| {
                std::fs::create_dir_all(tree().join("docs")).unwrap();
                std::fs::write(tree().join("docs/sample.docx"), docx_sample()).unwrap();
                mf::mount_file_render_html(db, MP_FS, "docs/sample.docx")
            }),
        ),
        (
            "render_html_docx_corrupt",
            Box::new(|db, _rt| {
                std::fs::create_dir_all(tree().join("docs")).unwrap();
                std::fs::write(tree().join("docs/bad.docx"), "not a zip at all").unwrap();
                mf::mount_file_render_html(db, MP_FS, "docs/bad.docx")
            }),
        ),
        (
            "render_html_md",
            Box::new(|db, _rt| mf::mount_file_render_html(db, MP_FS, "notes/alpha.md")),
        ),
        (
            "render_html_missing",
            Box::new(|db, _rt| mf::mount_file_render_html(db, MP_FS, "docs/missing.docx")),
        ),
        (
            "render_html_db_md",
            Box::new(|db, _rt| mf::mount_file_render_html(db, MP_DB, "notes/intro.md")),
        ),
        // === /P4.D381 ===
    ];

    // Planted for the whole run; `Drop` restores the denied parent's mode and
    // removes the tree even if a case panics (mirrors the oracle's try/finally).
    let _planted = PlantedBases::plant(PLANT_ROOT);

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut refused_rows = 0usize; // === P4.D381 ===
    let mut lines_compared = 0usize; // === P4.D381 ===
    let _ = all_thread_lines(); // === P4.D381: installed before any callsite ===
    for (name, run) in &cases {
        let want = oracle
            .get(*name)
            .unwrap_or_else(|| panic!("no oracle row {name}"));
        let (db, scratch) = fresh_db_with_base(&pepper, name, planted_base_for(name).as_deref());
        TREE.with(|t| *t.borrow_mut() = Some(scratch.join("fs-tree"))); // === P4.D381 ===
        all_thread_lines().lock().unwrap().clear(); // === P4.D381 ===
        let resp = run(&db, &rt);
        let (mut status, mut body) = response_to_status_body(resp);
        let tables = dump_tables(&db);
        // === P4.D381: the on-disk tree, and the `DOCX_RENDER_HTML_REFUSED`
        // both-ways table (each row asserted EXACTLY on both sides, then the
        // rest of the row — tables, tree — compared strictly) ===
        let tree_listing = list_tree(&scratch.join("fs-tree"));
        if let Some((_, v4_status, v4_body, v5_status, v5_body)) =
            DOCX_RENDER_HTML_REFUSED.iter().find(|r| r.0 == *name)
        {
            let v4_body: serde_json::Value = serde_json::from_str(v4_body).unwrap();
            let v5_body: serde_json::Value = serde_json::from_str(v5_body).unwrap();
            assert_eq!(
                (want["status"].as_u64(), &want["body"]),
                (Some(*v4_status as u64), &v4_body),
                "DOCX_RENDER_HTML_REFUSED {name}: v4 no longer answers the recorded arm"
            );
            assert_eq!(
                (status, &body),
                (*v5_status as i64, &v5_body),
                "DOCX_RENDER_HTML_REFUSED {name}: v5 no longer answers the named refusal"
            );
            refused_rows += 1;
            status = want["status"].as_i64().unwrap();
            body = want["body"].clone();
        }
        // The lines, IN ORDER (the refused DOCX rows excepted: v4 logs its
        // render failure, v5 its refusal — the table above owns them).
        let captured = all_thread_lines().lock().unwrap().clone();
        if !DOCX_RENDER_HTML_REFUSED.iter().any(|r| r.0 == *name) {
            let (got_lines, want_lines) = (v5_lines(&captured), v4_lines(want));
            if got_lines != want_lines {
                failures.push(format!(
                    "case {name} lines:\n  v4 {want_lines:#?}\n  v5 {got_lines:#?}"
                ));
            }
            lines_compared += want_lines.len();
        }
        let mut want = want.clone();
        want.as_object_mut().unwrap().remove("logs");
        let want = &want;
        // === /P4.D381 ===
        let got = json!({ "name": name, "status": status, "body": body, "tables": tables, "tree": tree_listing });
        // P4.D381: collect every red row by id (the red-first record needs
        // the whole set, not the first divergence).
        if norm(&got) != norm(want) {
            failures.push(format!(
                "case {name}
{}",
                diff_first_line(&norm(&got), &norm(want))
            ));
        }
        drop(db);
        let _ = std::fs::remove_dir_all(&scratch);
        checked += 1;
    }

    assert!(
        failures.is_empty(),
        "{} row(s) diverge:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 75, "expected the 75 mount-ops cases");
    assert_eq!(
        refused_rows,
        DOCX_RENDER_HTML_REFUSED.len(),
        "every DOCX_RENDER_HTML_REFUSED row ran"
    ); // === P4.D381 ===
    assert!(lines_compared >= 30, "only {lines_compared} lines compared"); // === P4.D381 ===
    eprintln!("OK: mount-ops matched oracle ({checked} cases).");
}

/// The P4.6y tier-3 refusal arms: convert/deconvert exist as variants, run
/// v4's capability guards LIVE, and refuse the conversion itself loudly
/// (no oracle — v4 would really convert; the refusal is the v5 contract).
#[test]
fn convert_deconvert_refusal_arms() {
    let pepper = test_pepper();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (db, scratch) = fresh_db(&pepper, "convert-refusals");

    // convert: guards live...
    let r = rt.block_on(mf::mount_convert(&db, MP_DB));
    match &r {
        Response::Error(e) => {
            assert!(
                e.message.contains("already database-backed"),
                "{}",
                e.message
            )
        }
        other => panic!("expected guard error, got {other:?}"),
    }
    let r = rt.block_on(mf::mount_convert(&db, BOGUS));
    match &r {
        Response::Error(e) => assert!(e.message.contains("not found"), "{}", e.message),
        other => panic!("expected 404, got {other:?}"),
    }
    // ...and the refusal itself names the deferring order.
    let r = rt.block_on(mf::mount_convert(&db, MP_FS));
    match &r {
        Response::Error(e) => {
            assert!(
                e.message.contains("recognized but not yet available"),
                "{}",
                e.message
            );
            assert!(e.message.contains("P4.6y"), "{}", e.message);
        }
        other => panic!("expected refusal, got {other:?}"),
    }

    // deconvert mirrors.
    let r = rt.block_on(mf::mount_deconvert(&db, MP_DB, ""));
    match &r {
        Response::Error(e) => assert!(e.message.contains("non-empty targetPath"), "{}", e.message),
        other => panic!("expected 400, got {other:?}"),
    }
    let r = rt.block_on(mf::mount_deconvert(&db, MP_FS, "/tmp/x"));
    match &r {
        Response::Error(e) => {
            assert!(e.message.contains("not database-backed"), "{}", e.message)
        }
        other => panic!("expected guard error, got {other:?}"),
    }
    let r = rt.block_on(mf::mount_deconvert(&db, MP_DB, "/tmp/x"));
    match &r {
        Response::Error(e) => {
            assert!(
                e.message.contains("recognized but not yet available"),
                "{}",
                e.message
            )
        }
        other => panic!("expected refusal, got {other:?}"),
    }

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
    eprintln!("OK: convert/deconvert refusal arms pinned.");
}
