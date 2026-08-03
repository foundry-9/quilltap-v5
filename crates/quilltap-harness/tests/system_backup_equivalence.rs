//! P4.9G5 backup differential: runs the Rust collector + manifest + staging
//! (`quilltap_core::services::backup`) over a FRESH copy of the committed
//! `system-data-*` fixture family and diffs the **extracted archive tree**
//! against v4's REAL `createBackup` (the `system-backup` oracle).
//!
//! ## Why the tree and not the zip bytes
//!
//! v4 shells out to `zip -r`; v5 uses a zip crate. Archive framing differs by
//! design, so the equivalence claim is the DB→disk projection: the relative path
//! set first, then, per file, the exact bytes (the JSON data files — v4's
//! `writeJsonArrayFile` layout is part of the contract) or sha256+size (binary).
//!
//! ## What is normalized, and nothing else
//!
//! - `manifest.json`'s `createdAt` (wall clock) and `appVersion` (v4 reads
//!   `process.env.npm_package_version` with a `'2.0.0'` fallback — an
//!   environment property; v5 carries its own crate version and the host passes
//!   it in).
//! - `data/characters.json`'s **nested** `physicalDescription.createdAt` /
//!   `.updatedAt`: the vault overlay synthesizes the default physical
//!   description at read time and stamps it "now" on both sides. Its `id` is
//!   derived, not random, and IS compared. The normalizer keys off indentation
//!   (six spaces = one level inside the character object), so the character's
//!   own timestamps stay under diff.
//!
//! ## Four files are compared order-insensitively (recorded divergence)
//!
//! `characters.json`, `chats.json`, `projects.json` and `groups.json` come from
//! v5's store/vault OVERLAY readers (and, for the chats' inline message events,
//! `chats_messages_read::get_messages`). Those readers emit exactly v4's field
//! SET with exactly v4's values, but in a different key ORDER: v4 merges
//! slim-row → managed fields → store properties, v5 merges slim-row → properties
//! → managed. No prior differential could see it — `serde_json`'s
//! `preserve_order` map is an `IndexMap`, whose `PartialEq` is order-independent,
//! so every object-level diff in the repo compares as a set. It surfaces here
//! only because an archive diff is byte-level.
//!
//! Key order in a JSON object carries no meaning, and restore reads these files
//! by key, so this is left as a recorded divergence rather than a reshuffle of
//! shared overlay readers. Those four files are still diffed for content AND
//! formatting: both sides are parsed, canonicalized to sorted key order, and
//! re-rendered with the same pretty layout before comparison, so a real content
//! or indentation drift still fails. The other 34 data files stay byte-exact.
//!
//! ## The zip round-trip
//!
//! The `backup_full` case additionally runs the whole `create_backup`
//! orchestration over a test `BackupHost` and extracts the archive it produces:
//! the extracted tree must reproduce the directly-staged tree exactly, under a
//! single `quilltap-backup-<stamp>` root. That is the claim about the zip —
//! never its bytes.
//!
//! Generate the oracle (see `harness/oracle/cases/system-backup.test.ts`), then:
//!   QT_ORACLE_SYSTEM_BACKUP=/tmp/oracle-system-backup.ndjson \
//!     cargo test -p quilltap-harness --test system_backup_equivalence -- --nocapture

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::backup::{
    collect_user_data, create_backup, create_manifest, stage_backup, BackupHost, HostCounts,
    HostDirs,
};
use quilltap_core::services::file_storage::StorageBackend;
use serde_json::Value;
use sha2::{Digest, Sha256};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const USER: &str = "e18e05bc-63e8-4539-8a85-719b7a508850";
/// Byte-identical to the oracle case's `IMAGE_BYTES`.
const IMAGE_BYTES: &[u8] = b"quilltap-fixture-portrait-bytes\n";
const IMAGE_STORAGE_KEY: &str = "e18e05bc-63e8-4539-8a85-719b7a508850/portrait.png";
const NORMALIZED: &str = "<normalized>";

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

/// The disk half of v4's `LocalFileStorageBackend`, scoped to one scratch root —
/// enough for the staging leg (download only).
struct ScratchBackend {
    root: PathBuf,
}

impl StorageBackend for ScratchBackend {
    fn upload(&self, _k: &str, _c: &[u8], _t: &str) -> Result<(), String> {
        Err("unused".into())
    }
    fn download(&self, key: &str) -> Result<Vec<u8>, String> {
        let mut p = self.root.clone();
        for part in key.split('/') {
            p.push(part);
        }
        std::fs::read(&p).map_err(|e| format!("Failed to download file '{key}': {e}"))
    }
    fn delete(&self, _k: &str) -> Result<(), String> {
        Err("unused".into())
    }
    fn exists(&self, key: &str) -> Result<bool, String> {
        Ok(self.download(key).is_ok())
    }
}

/// A [`BackupHost`] over the case's scratch root: the scratch storage backend,
/// a scratch temp dir, no plugins/themes directories, the pinned app version,
/// and a frozen clock. Its temp store is a plain cell — the single-use and TTL
/// behaviour is the host impl's own unit tests; here it only has to hand the
/// zip path back.
struct TestBackupHost {
    root: PathBuf,
    files_root: PathBuf,
    stored: std::sync::Mutex<Option<(String, PathBuf)>>,
}

impl BackupHost for TestBackupHost {
    fn storage(&self) -> std::sync::Arc<dyn StorageBackend> {
        std::sync::Arc::new(ScratchBackend {
            root: self.files_root.clone(),
        })
    }
    fn pixel_codec(&self) -> std::sync::Arc<dyn quilltap_core::services::file_storage::PixelCodec> {
        // Backup never transcodes — it stages the bytes it finds. (The RESTORE
        // side is where the codec matters; see `system_restore_state`.)
        std::sync::Arc::new(quilltap_core::services::file_storage::NotConfiguredPixelCodec)
    }
    fn temp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }
    fn host_dirs(&self) -> HostDirs {
        HostDirs::default()
    }
    fn app_version(&self) -> String {
        NORMALIZED.to_string()
    }
    fn now_ms(&self) -> i64 {
        0
    }
    fn store_backup(&self, backup_id: &str, zip_path: &Path) {
        *self.stored.lock().unwrap() = Some((backup_id.to_string(), zip_path.to_path_buf()));
    }
    fn take_backup(&self, _backup_id: &str) -> Option<PathBuf> {
        self.stored.lock().unwrap().take().map(|(_, p)| p)
    }
    // The RESTORE side's pending-upload store (added with unit 3). This case
    // never uploads — the backup direction is what it measures — so the three
    // are inert here; `system_restore_equivalence` exercises them.
    fn store_upload(&self, _upload_id: &str, _zip_path: &Path) {}
    fn get_upload(&self, _upload_id: &str) -> Option<PathBuf> {
        None
    }
    fn remove_upload(&self, _upload_id: &str) {}
}

/// Extract `zip_path` under `dest`, returning the single `quilltap-backup-*`
/// root's path (the shape v4's `extractZipToTemp` looks for).
fn unzip_to(zip_path: &Path, dest: &Path) -> PathBuf {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(zip_path).expect("open zip"))
        .expect("read archive");
    archive.extract(dest).expect("extract");
    let mut roots: Vec<PathBuf> = std::fs::read_dir(dest)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("quilltap-backup-"))
        })
        .collect();
    assert_eq!(
        roots.len(),
        1,
        "expected exactly one staging root in the zip"
    );
    roots.pop().unwrap()
}

struct Scratch {
    root: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fresh_scratch(tag: &str) -> Scratch {
    let root = std::env::temp_dir().join(format!("qt-sysbackup-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    Scratch { root }
}

fn open_fixture(scratch: &Path) -> Db {
    let main = scratch.join("main.db");
    let mount = scratch.join("mount.db");
    let llm = scratch.join("llm.db");
    let f = fixtures_dir();
    std::fs::copy(f.join("system-data-main.db"), &main).unwrap();
    std::fs::copy(f.join("system-data-mount.db"), &mount).unwrap();
    std::fs::copy(f.join("system-data-llmlogs.db"), &llm).unwrap();
    Db::open(
        DbPaths {
            main,
            mount_index: Some(mount),
            llm_logs: Some(llm),
        },
        TEST_PEPPER,
    )
    .expect("open fixture db")
}

/// Every regular file under `root`, `/`-joined and relative.
fn walk(root: &Path) -> Vec<String> {
    fn rec(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rec(root, &p, out);
            } else if p.is_file() {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    rec(root, root, &mut out);
    out.sort();
    out
}

/// Replace the value on every line whose indentation and key match, keeping the
/// trailing comma. Precise enough to leave sibling timestamps at other nesting
/// depths under diff.
fn blank_keys_at_indent(text: &str, indent: usize, keys: &[&str]) -> String {
    let pad = " ".repeat(indent);
    text.split('\n')
        .map(|line| {
            for k in keys {
                let prefix = format!("{pad}\"{k}\": \"");
                if line.starts_with(&prefix) {
                    let comma = if line.ends_with(',') { "," } else { "" };
                    return format!("{pad}\"{k}\": \"{NORMALIZED}\"{comma}");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The overlay-hydrated files whose key ORDER differs from v4 (see the header).
const ORDER_INSENSITIVE: &[&str] = &[
    "data/characters.json",
    "data/chats.json",
    "data/projects.json",
    "data/groups.json",
];

/// Recursively sort object keys so two orderings of the same content render
/// identically.
fn canonicalize(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize(&m[k]));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonicalize).collect()),
        other => other.clone(),
    }
}

fn normalize_text(path: &str, text: &str) -> String {
    let normalized = match path {
        "manifest.json" => blank_keys_at_indent(text, 2, &["createdAt", "appVersion"]),
        "data/characters.json" => blank_keys_at_indent(text, 6, &["createdAt", "updatedAt"]),
        _ => text.to_string(),
    };
    if ORDER_INSENSITIVE.contains(&path) {
        let parsed: Value = serde_json::from_str(&normalized)
            .unwrap_or_else(|e| panic!("{path} is not valid JSON: {e}"));
        return serde_json::to_string_pretty(&canonicalize(&parsed)).expect("serializable");
    }
    normalized
}

/// A staged entry. `size` is the file's RAW byte length and is only compared
/// for binary payloads — a normalized text file's length legitimately changes
/// when a timestamp is replaced, so for text the normalized bytes are the
/// comparison.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    text: Option<String>,
    sha256: Option<String>,
    size: Option<usize>,
}

fn describe(root: &Path) -> BTreeMap<String, Entry> {
    walk(root)
        .into_iter()
        .map(|rel| {
            let bytes = std::fs::read(root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)))
                .expect("read staged file");
            let is_text = rel == "manifest.json" || rel.starts_with("data/");
            let entry = if is_text {
                Entry {
                    text: Some(normalize_text(&rel, &String::from_utf8_lossy(&bytes))),
                    sha256: None,
                    size: None,
                }
            } else {
                Entry {
                    text: None,
                    sha256: Some(hex::encode(Sha256::digest(&bytes))),
                    size: Some(bytes.len()),
                }
            };
            (rel, entry)
        })
        .collect()
}

fn oracle_entries(tree: &[Value]) -> BTreeMap<String, Entry> {
    tree.iter()
        .map(|e| {
            let path = e["path"].as_str().unwrap().to_string();
            let text = e
                .get("text")
                .and_then(Value::as_str)
                .map(|t| normalize_text(&path, t));
            let sha256 = e.get("sha256").and_then(Value::as_str).map(str::to_string);
            let size = sha256
                .as_ref()
                .map(|_| e["size"].as_u64().unwrap() as usize);
            let entry = Entry { text, sha256, size };
            (path, entry)
        })
        .collect()
}

/// Show the first differing line of two texts (a 20 000-line data file's diff
/// is otherwise unreadable).
fn first_diff(a: &str, b: &str) -> String {
    let al: Vec<&str> = a.split('\n').collect();
    let bl: Vec<&str> = b.split('\n').collect();
    for (i, (la, lb)) in al.iter().zip(bl.iter()).enumerate() {
        if la != lb {
            let from = i.saturating_sub(3);
            let ctx = |lines: &[&str]| {
                lines[from..(i + 4).min(lines.len())]
                    .iter()
                    .enumerate()
                    .map(|(k, l)| format!("      {}{}", if from + k == i { "> " } else { "  " }, l))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            return format!(
                "line {}:\n    rust:\n{}\n    oracle:\n{}",
                i + 1,
                ctx(&al),
                ctx(&bl)
            );
        }
    }
    format!(
        "identical for {} lines; lengths differ (rust {} vs oracle {})",
        a.split('\n').count().min(b.split('\n').count()),
        a.len(),
        b.len()
    )
}

fn copy_tree(src: &Path, dest: &Path) {
    let _ = std::fs::create_dir_all(dest);
    let Ok(entries) = std::fs::read_dir(src) else {
        return;
    };
    for e in entries.flatten() {
        let from = e.path();
        let to = dest.join(e.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            let _ = std::fs::copy(&from, &to);
        }
    }
}

#[test]
fn system_backup_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_SYSTEM_BACKUP") else {
        eprintln!("SKIP: QT_ORACLE_SYSTEM_BACKUP unset");
        return;
    };
    let raw = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let cases: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!cases.is_empty(), "oracle produced no cases");

    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let seed_file_bytes = name == "backup_full";

        let scratch = fresh_scratch(name);
        let db = open_fixture(&scratch.root);

        // The oracle seeds the user file's bytes into its data dir for the
        // `backup_full` case only; the other case proves warn-and-continue.
        let files_root = scratch.root.join("files");
        if seed_file_bytes {
            let mut p = files_root.clone();
            for part in IMAGE_STORAGE_KEY.split('/') {
                p.push(part);
            }
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, IMAGE_BYTES).unwrap();
        }
        let backend = ScratchBackend {
            root: files_root.clone(),
        };

        let data = collect_user_data(&db, USER).expect("collect");
        // No plugins/themes directory on this host — v4's `existsSync` guard
        // makes both counts 0 and stages neither subtree.
        let manifest = create_manifest(USER, &data, NORMALIZED, NORMALIZED, HostCounts::default());
        let staging = scratch.root.join("staging");
        let staged = stage_backup(
            &db,
            &backend,
            &data,
            &manifest,
            &staging,
            &HostDirs::default(),
        )
        .expect("stage");

        // ⚠ DELIBERATE DIVERGENCE (dogfood #59) — v5 REPORTS what it could not
        // stage. v4 warns to its module logger and forgets, so the operator only
        // learns at restore time (`File not found in backup: <name>`), which is
        // exactly how the 2026-08-03 walk found 19 of them. There is nothing to
        // diff against here — v4 has no analogue — so the two arms are asserted
        // directly, in both directions: the healthy case must report NOTHING (so
        // the response key stays absent and the body byte-identical to v4's), and
        // the missing-bytes case must name the file.
        if seed_file_bytes {
            assert!(
                staged.skipped_files.is_empty(),
                "[{name}] a healthy backup must report no skipped files, got {:?}",
                staged.skipped_files
            );
        } else {
            assert_eq!(
                staged.skipped_files,
                vec!["portrait.png".to_string()],
                "[{name}] the file whose bytes are gone must be named"
            );
        }

        // Debugging aid: `QT_BACKUP_DUMP=<dir>` copies each case's staged tree
        // out before the scratch is dropped.
        if let Ok(dump) = std::env::var("QT_BACKUP_DUMP") {
            let dest = PathBuf::from(dump).join(name);
            let _ = std::fs::remove_dir_all(&dest);
            copy_tree(&staging, &dest);
        }

        // 0. The zip round-trip, on the full case only: `create_backup` collects,
        //    stages, and zips; extracting the archive must reproduce the staged
        //    tree byte-for-byte, under a single `quilltap-backup-<stamp>` root.
        if seed_file_bytes {
            let host = TestBackupHost {
                root: scratch.root.clone(),
                files_root: files_root.clone(),
                stored: std::sync::Mutex::new(None),
            };
            let created =
                create_backup(&db, &host, USER, "2026-03-01T00:00:00.000Z").expect("create_backup");
            assert!(
                created
                    .zip_path
                    .ends_with("quilltap-backup-2026-03-01T00-00-00-000Z.zip"),
                "archive name follows v4's `[:.]`→`-` stamp: {:?}",
                created.zip_path
            );
            let out = scratch.root.join("unzipped");
            let root = unzip_to(&created.zip_path, &out);
            let from_zip = describe(&root);
            let direct = describe(&staging);
            if from_zip != direct {
                let zk: Vec<&String> = from_zip.keys().collect();
                let dk: Vec<&String> = direct.keys().collect();
                failures.push(format!(
                    "[{name}] the zip round-trip did not reproduce the staged tree\n  \
                     zip paths: {zk:?}\n  staged paths: {dk:?}"
                ));
            } else {
                println!(
                    "OK {name}: zip round-trip reproduced {} entries",
                    from_zip.len()
                );
            }
        }

        // 1. The manifest object.
        let mut expected_manifest = case["manifest"].clone();
        expected_manifest["createdAt"] = Value::String(NORMALIZED.into());
        expected_manifest["appVersion"] = Value::String(NORMALIZED.into());
        if manifest != expected_manifest {
            failures.push(format!(
                "[{name}] manifest differs\n  rust:   {}\n  oracle: {}",
                serde_json::to_string(&manifest).unwrap(),
                serde_json::to_string(&expected_manifest).unwrap()
            ));
        }

        // 2. The archive tree: paths first, then contents.
        let got = describe(&staging);
        let want = oracle_entries(case["tree"].as_array().unwrap());
        let got_paths: Vec<&String> = got.keys().collect();
        let want_paths: Vec<&String> = want.keys().collect();
        if got_paths != want_paths {
            let missing: Vec<_> = want_paths
                .iter()
                .filter(|p| !got.contains_key(**p))
                .collect();
            let extra: Vec<_> = got_paths
                .iter()
                .filter(|p| !want.contains_key(**p))
                .collect();
            failures.push(format!(
                "[{name}] archive tree paths differ\n  missing: {missing:?}\n  extra: {extra:?}"
            ));
        }
        for (p, w) in &want {
            let Some(g) = got.get(p) else { continue };
            if g == w {
                continue;
            }
            let detail = match (&g.text, &w.text) {
                (Some(a), Some(b)) => first_diff(a, b),
                _ => format!("rust {g:?}\noracle {w:?}"),
            };
            failures.push(format!("[{name}] {p} differs\n  {detail}"));
        }
        if failures.is_empty() {
            println!("OK {name}: {} archive entries", want.len());
        }
    }

    assert!(
        failures.is_empty(),
        "{} backup difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
