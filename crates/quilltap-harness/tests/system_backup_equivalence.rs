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
    // P4.111 widened the committed `system-data-*.db` to carry the
    // P4.D171 columns natively (`pragma_table_info` proof: P4.117 lane
    // record) — the `ensure_*_column` heals that used to run here are dead
    // and removed.
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

/// P4.149: the oracle's `BLOB_PROFILE_ID`.
const BLOB_PROFILE_ID: &str = "c0ffee00-0000-4000-8000-0000000000b2";

/// P4.149: the oracle's `plantBlobProfile`, statement for statement.
fn plant_blob_profile(db: &Db) {
    db.write_blocking(|w| {
        let c = w.main().connection();
        c.execute(
            "CREATE TEMP TABLE qt_blob_profile AS SELECT * FROM connection_profiles \
             WHERE userId = ?1 ORDER BY id LIMIT 1",
            [USER],
        )?;
        c.execute(
            "UPDATE qt_blob_profile SET id = ?1, name = x'4a554e4b', isDefault = 0",
            [BLOB_PROFILE_ID],
        )?;
        let n = c.execute(
            "INSERT INTO connection_profiles SELECT * FROM qt_blob_profile",
            [],
        )?;
        assert_eq!(n, 1, "the plant must clone one profile");
        c.execute_batch("DROP TABLE qt_blob_profile")?;
        Ok(())
    })
    .expect("plant the BLOB-named profile");
}

/// P4.106 item 6 — the oracle's `plantInforms`, cell for cell: a PENDING and a
/// CONSUMED row on each of the user's first two chats (sorted by id), on the
/// per-run COPY only.
fn plant_informs(db: &Db) {
    use quilltap_core::db::chat_informs::{ChatInformCreate, ChatInformsRepository};
    db.write_blocking(|w| {
        let conn = w.main().connection();
        quilltap_core::db::chat_informs::ensure_chat_informs_table(conn)?;
        let mut stmt = conn.prepare("SELECT id FROM chats WHERE userId = ?1 ORDER BY id")?;
        let chats: Vec<String> = stmt
            .query_map([USER], |r| r.get::<_, String>(0))?
            .collect::<Result<_, _>>()?;
        assert!(chats.len() >= 2, "inform plant needs two chats");
        let repo = ChatInformsRepository::new(conn);
        for (i, chat) in chats.iter().take(2).enumerate() {
            let n = i + 1;
            for consumed in [false, true] {
                let k = if consumed { 2 } else { 1 };
                repo.create(&ChatInformCreate {
                    id: format!("1c0000{n}{k}-0000-4000-8000-00000000000{k}"),
                    chat_id: chat.clone(),
                    batch_id: format!("1c0000{n}{k}-0000-4000-8000-0000000000b{k}"),
                    participant_id: format!("1c0000{n}0-0000-4000-8000-0000000000a{n}"),
                    content_markdown: if consumed {
                        format!("Consumed passage {n}.")
                    } else {
                        format!("Pending passage {n}.")
                    },
                    record_message_id: None,
                    // P4.D249: ONE standing row (the oracle's
                    // `permanent: n === 1 && consumed`).
                    permanent: n == 1 && consumed,
                    created_at: "2026-01-02T00:00:00.000Z".into(),
                    updated_at: if consumed {
                        "2026-01-03T00:00:00.000Z".into()
                    } else {
                        "2026-01-02T00:00:00.000Z".into()
                    },
                    consumed_at: consumed.then(|| "2026-01-03T00:00:00.000Z".to_string()),
                    consumed_by_message_id: consumed
                        .then(|| format!("1c0000{n}{k}-0000-4000-8000-0000000000d{k}")),
                })?;
            }
        }
        Ok(())
    })
    .expect("plant chat_informs on the fixture copy");
}

/// ## ⚠ RULED DIVERGENCE — `FIND_ALL_DROPS_NULLABLE` (the human, 2026-10-08:
/// FIX v5, file v4 — ruled at P4.D255, whose record names this lane's backup)
///
/// v4's backup reads the ledger through the base repository's `findAll`
/// (`backup-service.ts:298`), whose SQLite backend maps every NULL column to
/// `undefined` (`backends/sqlite/backend.ts:466-467`, "for Zod .optional()
/// compatibility") — and `WardrobeWearStatsRowSchema`'s `wearerCharacterId` /
/// `lastWornChatId` are `z.string().nullable()`, NOT optional, so `validateSafe`
/// REFUSES the row and `findAll` drops it. Every UNATTRIBUTED tally (a departed
/// wearer's fold) and every chat-less tally is silently missing from a v4
/// backup (measured on this family's plant, 2026-10-08: three rows planted
/// through v4's real `upsertRows`, ONE in `data/wardrobe-wear.json`, count 1).
/// v4's own `wardrobe-wear-backup.test.ts:211-217` mocks `findAll` and never
/// sees it. v5's C1 `find_all` carries the WHOLE table. Pinned both ways: the
/// (v5) arm requires the carved rows in v5's archive; the (v4) arm requires
/// them ABSENT from v4's, so the day v4 fixes its read the family reddens "v4
/// converged". Every other byte of the file and the manifest is compared after
/// the carve.
const FIND_ALL_DROPS_NULLABLE: &[&str] = &[
    // wearerCharacterId: null (and lastWornChatId: null)
    "3e000002-0000-4000-8000-000000000002",
    // lastWornChatId: null
    "3e000003-0000-4000-8000-000000000003",
];

/// P4.D264 — the oracle's `plantWardrobeWear`, cell for cell: the table
/// through C1 §6's helper (the migration's DDL, as v4's plant runs
/// `WARDROBE_WEAR_STATS_DDL`), then the two rows through C1 §3's `upsert_rows`.
fn plant_wardrobe_wear(db: &Db) {
    use quilltap_core::db::wardrobe_wear_stats::{
        WardrobeWearStatsRepository, WardrobeWearStatsRow,
    };
    let row = |id: &str,
               wearer: Option<&str>,
               count: i64,
               first: &str,
               last: &str,
               chat: Option<&str>| {
        WardrobeWearStatsRow {
            id: id.into(),
            item_id: "ac000000-0000-4000-8000-000000000001".into(),
            wearer_character_id: wearer.map(str::to_string),
            wear_count: count,
            first_worn_at: first.into(),
            last_worn_at: last.into(),
            last_worn_chat_id: chat.map(str::to_string),
            created_at: first.into(),
            updated_at: last.into(),
        }
    };
    let rows = vec![
        row(
            "3e000001-0000-4000-8000-000000000001",
            Some("a1000000-0000-4000-8000-000000000001"),
            3,
            "2026-03-02T00:00:00.000Z",
            "2026-03-05T00:00:00.000Z",
            Some("c1000000-0000-4000-8000-000000000001"),
        ),
        row(
            "3e000002-0000-4000-8000-000000000002",
            None,
            2,
            "2026-03-01T00:00:00.000Z",
            "2026-03-03T00:00:00.000Z",
            None,
        ),
        row(
            "3e000003-0000-4000-8000-000000000003",
            Some("a1000000-0000-4000-8000-000000000002"),
            1,
            "2026-03-04T00:00:00.000Z",
            "2026-03-04T00:00:00.000Z",
            None,
        ),
    ];
    db.write_blocking(move |w| {
        let conn = w.main().connection();
        quilltap_core::test_support::ensure_wear_ledger_on(conn);
        WardrobeWearStatsRepository::new(conn).upsert_rows(&rows)?;
        Ok(())
    })
    .expect("plant wardrobe_wear_stats on the fixture copy");
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
        let seed_file_bytes = name == "backup_full"
            || name == "backup_compact"
            || name == "backup_with_informs"
            || name == "backup_with_blob_profile"
            || name == "backup_with_wardrobe_wear";
        // [P4.D46] `backup_compact` runs the same projection with the compact
        // flag: memory embeddings nulled, the six derived embedding data files
        // ABSENT from the tree (asserted below and by the oracle tree diff),
        // manifest.compact stamped.
        let compact = name == "backup_compact";

        let scratch = fresh_scratch(name);
        let db = open_fixture(&scratch.root);
        if name == "backup_with_informs" {
            plant_informs(&db);
        }
        if name == "backup_with_blob_profile" {
            plant_blob_profile(&db);
        }
        if name == "backup_with_wardrobe_wear" {
            plant_wardrobe_wear(&db);
        }

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

        let collected = collect_user_data(&db, USER).expect("collect");
        let data = if compact {
            quilltap_core::services::backup::collect::compact_backup_data(collected)
        } else {
            collected
        };
        // No plugins/themes directory on this host — v4's `existsSync` guard
        // makes both counts 0 and stages neither subtree.
        let manifest = create_manifest(
            USER,
            &data,
            NORMALIZED,
            NORMALIZED,
            HostCounts::default(),
            compact,
        );
        let staging = scratch.root.join("staging");
        let staged = stage_backup(
            &db,
            &backend,
            &data,
            &manifest,
            &staging,
            &HostDirs::default(),
            compact,
        )
        .expect("stage");

        if compact {
            // Absent, not empty — absence is what shrinks the archive; the
            // oracle tree diff holds the full claim, this keeps it legible.
            for f in [
                "embedding-status.json",
                "conversation-chunks.json",
                "tfidf-vocabularies.json",
                "vector-index-metas.json",
                "vector-entries.json",
                "doc-mount-chunks.json",
            ] {
                assert!(
                    !staging.join("data").join(f).exists(),
                    "[{name}] compact must OMIT data/{f}"
                );
            }
            assert_eq!(manifest["compact"], serde_json::Value::Bool(true));
        } else {
            assert!(
                manifest.get("compact").is_none(),
                "[{name}] a full backup's manifest must not carry the compact key"
            );
        }

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

        // P4.106 item 6 — non-vacuity for the plant: the staged
        // `data/chat-informs.json` carries all four planted rows (pending AND
        // consumed), and the manifest counts them. The tree diff below is what
        // compares the bytes to v4's.
        if name == "backup_with_informs" {
            let text = std::fs::read_to_string(staging.join("data/chat-informs.json"))
                .expect("data/chat-informs.json staged");
            let rows: Vec<Value> = serde_json::from_str(&text).expect("chat-informs.json");
            assert_eq!(rows.len(), 4, "[{name}] the four planted informs: {text}");
            assert_eq!(
                manifest["counts"]["chatInforms"],
                Value::from(4),
                "[{name}] the manifest's chatInforms count"
            );
        }

        // P4.D264 — non-vacuity for the plant: the staged
        // `data/wardrobe-wear.json` carries BOTH rows (the unattributed one
        // included) and the manifest counts them; every OTHER case stages the
        // file too, as `[]` (v4 writes it on every archive).
        let wear_text = std::fs::read_to_string(staging.join("data/wardrobe-wear.json"))
            .expect("data/wardrobe-wear.json staged on every archive");
        let wear_rows: Vec<Value> = serde_json::from_str(&wear_text).expect("wardrobe-wear.json");
        let want_rows = if name == "backup_with_wardrobe_wear" {
            3
        } else {
            0
        };
        assert_eq!(
            wear_rows.len(),
            want_rows,
            "[{name}] wardrobe-wear.json: {wear_text}"
        );
        assert_eq!(
            manifest["counts"]["wardrobeWear"],
            Value::from(want_rows),
            "[{name}] the manifest's wardrobeWear count"
        );

        // P4.149 (Ruling R-C) — non-vacuity for the plant: the clone IS in the
        // table, and NOT in the staged `data/connection-profiles.json` (v4's
        // `validateSafe` drop). The tree diff below compares the bytes to v4's.
        if name == "backup_with_blob_profile" {
            let in_table: i64 = db
                .read_main(|c| {
                    Ok(c.query_row(
                        "SELECT COUNT(*) FROM connection_profiles WHERE id = ?1",
                        [BLOB_PROFILE_ID],
                        |r| r.get(0),
                    )?)
                })
                .expect("count the plant");
            assert_eq!(in_table, 1, "[{name}] the BLOB-named clone must be planted");
            let text = std::fs::read_to_string(staging.join("data/connection-profiles.json"))
                .expect("data/connection-profiles.json staged");
            assert!(
                !text.contains(BLOB_PROFILE_ID),
                "[{name}] the BLOB-named profile must be dropped from the archive"
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
            let created = create_backup(&db, &host, USER, "2026-03-01T00:00:00.000Z", compact)
                .expect("create_backup");
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

        // P4.D264 — the `FIND_ALL_DROPS_NULLABLE` carve (see the const).
        let mut got = describe(&staging);
        let mut manifest = manifest;
        if name == "backup_with_wardrobe_wear" {
            let rows_of =
                |text: &str| -> Vec<Value> { serde_json::from_str(text).expect("wear rows") };
            let id_of = |r: &Value| r["id"].as_str().unwrap_or_default().to_string();
            let v5_rows = rows_of(got["data/wardrobe-wear.json"].text.as_deref().unwrap());
            let v4_rows: Vec<Value> = case["tree"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["path"] == "data/wardrobe-wear.json")
                .and_then(|e| e["text"].as_str())
                .map(rows_of)
                .unwrap_or_default();
            for carved in FIND_ALL_DROPS_NULLABLE {
                if !v5_rows.iter().any(|r| id_of(r) == *carved) {
                    failures.push(format!(
                        "[{name}] FIND_ALL_DROPS_NULLABLE (v5): {carved} missing from v5's archive"
                    ));
                }
                if v4_rows.iter().any(|r| id_of(r) == *carved) {
                    failures.push(format!(
                        "[{name}] FIND_ALL_DROPS_NULLABLE (v4): {carved} is in v4's archive — v4 converged; retire the divergence"
                    ));
                }
            }
            let kept: Vec<Value> = v5_rows
                .into_iter()
                .filter(|r| !FIND_ALL_DROPS_NULLABLE.contains(&id_of(r).as_str()))
                .collect();
            let carved_file = scratch.root.join("wardrobe-wear.carved.json");
            quilltap_core::services::backup::staging::write_json_array_file(&carved_file, &kept)
                .expect("render the carved ledger");
            let text = std::fs::read_to_string(&carved_file).unwrap();
            got.get_mut("data/wardrobe-wear.json").unwrap().text = Some(text);
            manifest["counts"]["wardrobeWear"] = Value::from(kept.len());
            if let Some(m) = got.get_mut("manifest.json") {
                if let Some(t) = m.text.as_mut() {
                    *t = t.replace(
                        &format!("\"wardrobeWear\": {want_rows}"),
                        &format!("\"wardrobeWear\": {}", kept.len()),
                    );
                }
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
