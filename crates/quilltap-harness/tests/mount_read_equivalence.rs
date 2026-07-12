//! P4.6v mount READ + LIST differential: `services::mount_index::read_file` +
//! `::list` vs v4's REAL `readMountFile` + the files-list route body, over a
//! per-side COPY of the committed mounts fixture. The fs/obsidian mounts' sentinel
//! basePath is rewritten to a per-side copy of `mounts-fs-tree/`; fs-mount read
//! mtimes are normalized to 0 (nondeterministic copy time).
//!
//! Generate the oracle (Node 24, from the v4 checkout — see mount-read.ts header):
//!   QT_FIXTURE_MOUNTS_MAIN=.../mounts-main.db \
//!   QT_FIXTURE_MOUNTS_MOUNT=.../mounts-mount.db \
//!   QT_MOUNTS_FS_TREE=.../mounts-fs-tree \
//!     node --import tsx harness/oracle/cases/mount-read.ts > /tmp/oracle-mount-read.ndjson
//! Run:
//!   QT_ORACLE_MOUNT_READ=/tmp/oracle-mount-read.ndjson \
//!     cargo test -p quilltap-harness --test mount_read_equivalence

use std::path::PathBuf;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use quilltap_core::services::mount_index::list::mount_files_list;
use quilltap_core::services::mount_index::read_file::{
    read_mount_file, FileEncoding, MountFileError, ReadMountFileOptions,
};
use serde::Deserialize;
use serde_json::{json, Value};

const MP_DB: &str = "b6000000-0000-4000-8000-000000000001";
const MP_EMPTY: &str = "b6000000-0000-4000-8000-000000000002";
const MP_FS: &str = "b6000000-0000-4000-8000-000000000003";
const MP_OBS: &str = "b6000000-0000-4000-8000-000000000004";
const BOGUS: &str = "b6000000-0000-4000-8000-0000000000ee";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    test_pepper_base64: String,
}

fn spec() -> Spec {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../harness/oracle/fixtures/mounts.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("spec")
}
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

fn canon_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
                    *v = Value::Number((f as i64).into());
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(canon_numbers),
        Value::Object(o) => o.iter_mut().for_each(|(_, x)| canon_numbers(x)),
        _ => {}
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let mut m = serde_json::Map::new();
            for k in keys {
                m.insert(k.clone(), sorted(&o[k]));
            }
            Value::Object(m)
        }
        _ => v.clone(),
    }
}
fn norm(v: &Value) -> String {
    let mut v = v.clone();
    canon_numbers(&mut v);
    serde_json::to_string_pretty(&sorted(&v)).unwrap()
}

/// Normalize a `list` body: sort `files` by relativePath, sort `folders`.
fn norm_list(v: &Value) -> Value {
    let mut out = v.clone();
    if let Some(files) = out.get_mut("files").and_then(|f| f.as_array_mut()) {
        files.sort_by(|a, b| {
            a["relativePath"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["relativePath"].as_str().unwrap_or_default())
        });
    }
    if let Some(folders) = out.get_mut("folders").and_then(|f| f.as_array_mut()) {
        folders.sort_by(|a, b| {
            a.as_str()
                .unwrap_or_default()
                .cmp(b.as_str().unwrap_or_default())
        });
    }
    out
}

fn fresh_db(pepper: &str, tag: &str) -> (Db, PathBuf) {
    let scratch = std::env::temp_dir().join(format!("qt-mread-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    std::fs::copy(fixtures_dir().join("mounts-main.db"), &main).unwrap();
    std::fs::copy(fixtures_dir().join("mounts-mount.db"), &mount).unwrap();

    // Per-side copy of the committed fs tree.
    let tree = scratch.join("fs-tree");
    copy_dir(&fixtures_dir().join("mounts-fs-tree"), &tree);

    // Rewrite the sentinel basePath on the fs/obsidian mounts to this side's tree.
    {
        let w = Writer::open_writable(&mount, pepper).expect("open mount writer");
        w.connection()
            .execute(
                "UPDATE doc_mount_points SET basePath = ?1 WHERE id IN (?2, ?3)",
                rusqlite::params![tree.to_str().unwrap(), MP_FS, MP_OBS],
            )
            .expect("rewrite basePath");
    }

    let db = Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: None,
        },
        pepper,
    )
    .expect("open db");
    (db, scratch)
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).unwrap();
        }
    }
}

fn err_code(e: &MountFileError) -> String {
    match e {
        MountFileError::FileOp(fe) => fe.code.as_str().to_string(),
        other => format!("{other}"),
    }
}

struct ReadCase {
    id: &'static str,
    mp: &'static str,
    path: &'static str,
    opts: ReadMountFileOptions,
    fs_norm: bool,
}

fn rc(
    id: &'static str,
    mp: &'static str,
    path: &'static str,
    opts: ReadMountFileOptions,
    fs_norm: bool,
) -> ReadCase {
    ReadCase {
        id,
        mp,
        path,
        opts,
        fs_norm,
    }
}
fn o(
    encoding: Option<FileEncoding>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> ReadMountFileOptions {
    ReadMountFileOptions {
        encoding,
        offset,
        limit,
    }
}

#[test]
fn mount_read_matches_oracle() {
    let path = match std::env::var("QT_ORACLE_MOUNT_READ") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("SKIP: set QT_ORACLE_MOUNT_READ to the oracle NDJSON (see test header).");
            return;
        }
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let mut oracle: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line).unwrap();
        oracle.insert(row["id"].as_str().unwrap().to_string(), row);
    }

    let spec = spec();
    let (db, scratch) = fresh_db(&spec.test_pepper_base64, "a");

    let read_cases = vec![
        rc(
            "db-doc-read",
            MP_DB,
            "notes/intro.md",
            o(None, None, None),
            false,
        ),
        rc(
            "db-doc-window",
            MP_DB,
            "notes/intro.md",
            o(None, Some(1), Some(2)),
            false,
        ),
        rc(
            "db-doc-window-over",
            MP_DB,
            "notes/intro.md",
            o(None, Some(0), Some(999)),
            false,
        ),
        rc(
            "db-txt-read",
            MP_DB,
            "reference.txt",
            o(None, None, None),
            false,
        ),
        rc(
            "db-blob-read",
            MP_DB,
            "images/logo.png",
            o(None, None, None),
            false,
        ),
        rc(
            "db-blob-force-utf8",
            MP_DB,
            "images/logo.png",
            o(Some(FileEncoding::Utf8), None, None),
            false,
        ),
        rc(
            "fs-md-read",
            MP_FS,
            "notes/alpha.md",
            o(None, None, None),
            true,
        ),
        rc(
            "fs-window",
            MP_FS,
            "notes/sub/deep.md",
            o(None, Some(2), Some(2)),
            true,
        ),
        rc(
            "fs-json-read",
            MP_FS,
            "data/config.json",
            o(None, None, None),
            true,
        ),
        rc(
            "fs-force-base64",
            MP_FS,
            "notes/beta.txt",
            o(Some(FileEncoding::Base64), None, None),
            true,
        ),
        rc(
            "obs-read",
            MP_OBS,
            "notes/alpha.md",
            o(None, None, None),
            true,
        ),
        rc(
            "fs-notfound",
            MP_FS,
            "missing.md",
            o(None, None, None),
            true,
        ),
        rc("db-notfound", MP_DB, "nope.md", o(None, None, None), false),
        rc("mount-notfound", BOGUS, "x.md", o(None, None, None), false),
    ];

    let mut checked = 0usize;
    for c in &read_cases {
        let want = oracle
            .get(c.id)
            .unwrap_or_else(|| panic!("no oracle row {}", c.id));
        let got = match read_mount_file(&db, c.mp, c.path, c.opts) {
            Ok(mut r) => {
                if c.fs_norm {
                    r.mtime = 0;
                }
                json!({ "ok": r.to_json() })
            }
            Err(e) => json!({ "err": err_code(&e) }),
        };
        assert_eq!(
            norm(&got),
            norm(&want["out"]),
            "read case {}\n{}",
            c.id,
            diff(&norm(&got), &norm(&want["out"]))
        );
        checked += 1;
    }

    for (id, mp) in [
        ("list-db", MP_DB),
        ("list-empty", MP_EMPTY),
        ("list-fs", MP_FS),
        ("list-obs", MP_OBS),
    ] {
        let want = oracle
            .get(id)
            .unwrap_or_else(|| panic!("no oracle row {id}"));
        let got = mount_files_list(&db, mp).expect("list").to_json();
        assert_eq!(
            norm(&norm_list(&got)),
            norm(&norm_list(&want["out"])),
            "list case {id}\n{}",
            diff(&norm(&norm_list(&got)), &norm(&norm_list(&want["out"])))
        );
        checked += 1;
    }

    drop(db);
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(checked, 18, "expected 18 cases");
    eprintln!("OK: mount-read matched oracle ({checked} cases).");
}

fn diff(got: &str, want: &str) -> String {
    let g: Vec<&str> = got.lines().collect();
    let w: Vec<&str> = want.lines().collect();
    for i in 0..g.len().max(w.len()) {
        let gi = g.get(i).copied().unwrap_or("<none>");
        let wi = w.get(i).copied().unwrap_or("<none>");
        if gi != wi {
            return format!("  GOT : {gi}\n  WANT: {wi}");
        }
    }
    "(identical)".to_string()
}
