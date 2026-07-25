//! P4.9G5 restore differential, part 2 — the **tier-2 DB-state** diff for
//! `mode: 'replace'`.
//!
//! Both sides restore the *same committed archive* into a *freshly provisioned,
//! empty instance*, then every table in all three partitions is dumped row by
//! row and compared. A row-COUNT map would pass on a graph whose foreign keys
//! all point at nothing; the row dump would not.
//!
//! ## Normalization — mechanical, and exactly two rules
//!
//! v4 stamps every restored row with the write clock and mints a fresh id
//! wherever `create` provisions something (the project's and group's official
//! stores, each character's vault, the storage keys the file phase writes). Both
//! are legitimately nondeterministic, and both are recognized by ORIGIN rather
//! than by a hand-maintained column list:
//!
//! - **a UUID that does not appear anywhere in the archive** is minted, and is
//!   replaced by `<minted-N>` in first-encounter order over a deterministic walk
//!   (tables sorted by name, rows in rowid = insertion order). Both sides insert
//!   in the same order, so the labels line up; if they ever stop lining up, that
//!   IS the difference and the diff says so.
//! - **an ISO-8601 timestamp that does not appear anywhere in the archive** is a
//!   write-clock stamp and becomes `<ts>`. An archive timestamp that survives
//!   (`llm_logs.createdAt`, `memories.occurredAt`, `doc_mount_file_links
//!   .lastModified`, …) is left alone and IS compared.
//!
//! Nothing else is touched. In particular no column is normalized by name, so a
//! port that dropped a timestamp or minted an id where v4 preserved one fails.
//!
//! ## ⚠ EXPECTED DIVERGENCES — two v4 bugs this differential found
//!
//! Running v4's REAL restore against v4's REAL backup of a modern instance shows
//! that **v4 cannot restore a format-3/4 archive's document stores or user
//! files**. Both are pinned in [`EXPECTED_DIVERGENCES`] so neither side can drift
//! unnoticed, exactly as the sparse-array blob divergence is pinned in
//! `system_import_equivalence`.
//!
//! 1. **Every `doc_mount_points` and `doc_mount_file_links` row fails to
//!    restore.** `dumpMountIndexTable` (`backup-service.ts:72`) is a RAW
//!    `SELECT *`, so the archive carries `includePatterns` as JSON *text* and
//!    `enabled` / `allowEmbed` as INTEGER 0/1 — and `restore.ts` feeds those
//!    straight into repository `create`s whose Zod schemas demand `string[]` and
//!    `boolean`. Every row is rejected. The folders, file rows, documents and
//!    chunks DO restore (their schemas have no array/boolean columns), so v4
//!    produces a document-store graph with content but no stores and no links —
//!    every character vault, project store and group store is unreachable.
//! 2. **No user file is restored.** `getFileFromExtractedBackup`
//!    (`archive.ts:334`) gates the `files/<storageKey>` lookup on
//!    `backupFormat === 2`, but a modern manifest declares `backupFormat: 4`, so
//!    the lookup is skipped, the legacy `files/<category>/<id>_<name>` path
//!    misses, and every file becomes a `File not found in backup:` warning.
//!
//! v5 restores all three families. That divergence is **not a lane decision** —
//! it is flagged for the same human ruling the sparse-array one got, and queued
//! for a v4-side fix. Until then this file asserts it in both directions: v4
//! must restore ZERO of each, and v5 must restore the archive's full count.
//!
//! Generate the oracle (see `harness/oracle/cases/system-restore.test.ts`), then:
//!   QT_ORACLE_SYSTEM_RESTORE=/tmp/oracle-system-restore.ndjson \
//!     cargo test -p quilltap-harness --test system_restore_state -- --nocapture

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::backup::restore::{restore, RestoreMode, RestoreSummary};
use quilltap_core::services::backup::{BackupHost, HostDirs};
use quilltap_core::services::file_storage::{PixelCodec, StorageBackend};
use quilltap_core::services::provisioning::{provision_fresh_instance, SINGLE_USER_ID};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// The same pepper `build-provision-oracle.ts` keys its fresh instance with.
const TEST_PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

/// The families v4 cannot restore from a format-3/4 archive (see the header).
/// Each entry is `(partition, table)`; the assertion is "v4 restored 0, v5
/// restored more than 0", and those tables are then excluded from the row diff.
const EXPECTED_DIVERGENCES: &[(&str, &str)] = &[
    ("mountIndex", "doc_mount_points"),
    ("mountIndex", "doc_mount_file_links"),
    ("main", "files"),
];

/// Tables the divergence above makes incomparable downstream: v5's file phase
/// writes real blobs and links through the mount-store bridge, v4's writes
/// nothing, so the blob/file rows those create differ in COUNT for the same
/// reason. Diffed for the divergence, not row by row.
const DIVERGENCE_DEPENDENTS: &[(&str, &str)] = &[
    ("mountIndex", "doc_mount_blobs"),
    ("mountIndex", "doc_mount_files"),
];

fn archives_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../quilltap-web/tests/fixtures/restore-archives")
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
    let root = std::env::temp_dir().join(format!("qt-restorestate-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    Scratch { root }
}

/// A [`BackupHost`] over a scratch root: the real host image codec (so a
/// restored bitmap takes the same transcode path a live upload takes), scratch
/// temp, and no plugins/themes directories — matching the oracle, whose fresh
/// instance has neither.
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
        HostDirs::default()
    }
    fn app_version(&self) -> String {
        "<normalized>".to_string()
    }
    fn now_ms(&self) -> i64 {
        0
    }
    fn store_backup(&self, _id: &str, _p: &Path) {}
    fn take_backup(&self, _id: &str) -> Option<PathBuf> {
        None
    }
    fn store_upload(&self, _id: &str, _p: &Path) {}
    fn get_upload(&self, _id: &str) -> Option<PathBuf> {
        None
    }
    fn remove_upload(&self, _id: &str) {}
}

/// Dump one partition table for table, in rowid (insertion) order. BLOBs become
/// `sha256:<hex>` — byte-compared without being carried.
fn dump_partition(conn: &Connection) -> BTreeMap<String, Vec<Value>> {
    let mut names: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    names.sort();

    let mut out = BTreeMap::new();
    for table in names {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let mut m = Map::new();
                for (i, name) in cols.iter().enumerate() {
                    let v = match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(f) => json!(f),
                        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                        ValueRef::Blob(b) => {
                            Value::String(format!("sha256:{}", hex::encode(Sha256::digest(b))))
                        }
                    };
                    m.insert(name.clone(), v);
                }
                Ok(Value::Object(m))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        out.insert(table, rows);
    }
    out
}

/// Every UUID-shaped and ISO-timestamp-shaped string anywhere in the archive's
/// parsed JSON — the "came from the archive, so it is data" set.
fn archive_literals(zip: &Path, temp_root: &Path) -> HashSet<String> {
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its literal set");
    let mut out = HashSet::new();
    let d = &extracted.data;
    let all: Vec<&Vec<Value>> = vec![
        &d.characters,
        &d.chats,
        &d.tags,
        &d.connection_profiles,
        &d.image_profiles,
        &d.embedding_profiles,
        &d.memories,
        &d.files,
        &d.prompt_templates,
        &d.roleplay_templates,
        &d.provider_models,
        &d.projects,
        &d.groups,
        &d.llm_logs,
        &d.plugin_configs,
        &d.chat_settings,
        &d.folders,
        &d.wardrobe_items,
        &d.character_plugin_data,
        &d.conversation_annotations,
        &d.chat_documents,
        &d.instance_settings,
        &d.embedding_status,
        &d.conversation_chunks,
        &d.tfidf_vocabularies,
        &d.vector_index_metas,
        &d.vector_entries,
        &d.doc_mount_points,
        &d.doc_mount_folders,
        &d.doc_mount_files,
        &d.doc_mount_file_links,
        &d.doc_mount_chunks,
        &d.doc_mount_documents,
        &d.doc_mount_blobs,
        &d.project_doc_mount_links,
        &d.group_doc_mount_links,
        &d.group_character_members,
        &d.text_replacement_rules,
    ];
    for coll in all {
        for row in coll {
            collect_strings(row, &mut out);
        }
    }
    collect_strings(&extracted.manifest, &mut out);
    out
}

fn collect_strings(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::String(s) => {
            if is_uuid(s) || is_iso(s) {
                out.insert(s.clone());
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect_strings(x, out)),
        _ => {}
    }
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` — the only timestamp shape either engine writes.
fn is_iso(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 24
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'.'
        && b[23] == b'Z'
        && b
            .iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16 | 19 | 23) || c.is_ascii_digit())
}

/// The two normalization rules, applied over the whole dump in walk order.
struct Normalizer {
    literals: HashSet<String>,
    minted: BTreeMap<String, String>,
}

impl Normalizer {
    fn new(literals: HashSet<String>) -> Self {
        Normalizer {
            literals,
            minted: BTreeMap::new(),
        }
    }

    fn value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => {
                if self.literals.contains(s) {
                    return v.clone();
                }
                if is_uuid(s) {
                    let next = self.minted.len();
                    let label = self
                        .minted
                        .entry(s.clone())
                        .or_insert_with(|| format!("<minted-{next}>"));
                    return Value::String(label.clone());
                }
                if is_iso(s) {
                    return Value::String("<ts>".to_string());
                }
                // A storage key embeds a minted blob id; normalize its parts.
                if s.contains('/') && s.split('/').any(is_uuid) {
                    let parts: Vec<String> = s
                        .split('/')
                        .map(|p| match self.value(&Value::String(p.to_string())) {
                            Value::String(x) => x,
                            _ => p.to_string(),
                        })
                        .collect();
                    return Value::String(parts.join("/"));
                }
                v.clone()
            }
            Value::Array(a) => Value::Array(a.iter().map(|x| self.value(x)).collect()),
            Value::Object(m) => {
                let mut out = Map::new();
                for (k, val) in m {
                    out.insert(k.clone(), self.value(val));
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

fn read_cases() -> Option<Vec<Value>> {
    let path = std::env::var("QT_ORACLE_SYSTEM_RESTORE").ok()?;
    let raw = std::fs::read_to_string(&path).expect("read oracle ndjson");
    Some(
        raw.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
            .collect(),
    )
}

fn archive_for(name: &str) -> &'static str {
    match name {
        "restore_replace" => "restore-archive.zip",
        "restore_legacy_archive" => "restore-archive-legacy.zip",
        "restore_minimal" => "restore-archive-minimal.zip",
        other => panic!("unknown restore case {other}"),
    }
}

/// Provision a fresh instance under `dir` and open it.
fn fresh_instance(dir: &Path) -> Db {
    std::fs::create_dir_all(dir).unwrap();
    provision_fresh_instance(dir, TEST_PEPPER).expect("provision fresh instance");
    Db::open(
        DbPaths {
            main: dir.join("quilltap.db"),
            mount_index: Some(dir.join("quilltap-mount-index.db")),
            llm_logs: Some(dir.join("quilltap-llm-logs.db")),
        },
        TEST_PEPPER,
    )
    .expect("open fresh instance")
}

/// The summary keys whose values the divergence moves. Compared by the
/// divergence assertion, not by equality.
const DIVERGENT_SUMMARY_KEYS: &[&str] = &["files", "docMountPoints", "docMountFileLinks"];

#[test]
fn system_restore_state_equivalence() {
    let Some(cases) = read_cases() else {
        eprintln!("SKIP: QT_ORACLE_SYSTEM_RESTORE unset");
        return;
    };

    let mut failures: Vec<String> = Vec::new();
    let mut seen = 0usize;

    for case in &cases {
        let name = case["name"].as_str().unwrap();
        if !name.starts_with("restore_") {
            continue;
        }
        seen += 1;

        let scratch = fresh_scratch(name);
        let zip = archives_dir().join(archive_for(name));
        assert!(zip.exists(), "missing committed archive fixture: {zip:?}");

        let instance = scratch.root.join("instance");
        let db = fresh_instance(&instance);
        let host = TestHost {
            root: scratch.root.clone(),
        };
        std::fs::create_dir_all(host.temp_dir()).unwrap();

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
            .expect("restore succeeded");

        // Dump AFTER dropping the Db so every writer transaction has committed.
        drop(db);
        let got_state = read_state(&instance);
        let want_state = &case["state"];

        let literals = archive_literals(&zip, &host.temp_dir());
        compare_case(
            name,
            &summary,
            case,
            &got_state,
            want_state,
            literals,
            &mut failures,
        );
    }

    assert_eq!(seen, 3, "expected all three restore cases in the oracle");
    assert!(
        failures.is_empty(),
        "{} restore-state difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn read_state(dir: &Path) -> BTreeMap<String, BTreeMap<String, Vec<Value>>> {
    let mut out = BTreeMap::new();
    for (label, file) in [
        ("main", "quilltap.db"),
        ("mountIndex", "quilltap-mount-index.db"),
        ("llmLogs", "quilltap-llm-logs.db"),
    ] {
        let conn = quilltap_core::db::Writer::open_writable(&dir.join(file), TEST_PEPPER)
            .expect("reopen partition");
        out.insert(label.to_string(), dump_partition(conn.connection()));
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn compare_case(
    name: &str,
    summary: &RestoreSummary,
    case: &Value,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    literals: HashSet<String>,
    failures: &mut Vec<String>,
) {
    // ── 1. The summary counters, minus the divergent keys and `warnings`. ────
    let got_summary = serde_json::to_value(summary).expect("summary serializes");
    let want_summary = &case["summary"];
    for (k, gv) in got_summary.as_object().unwrap() {
        if k == "warnings" || DIVERGENT_SUMMARY_KEYS.contains(&k.as_str()) {
            continue;
        }
        let wv = &want_summary[k];
        if gv != wv {
            failures.push(format!("[{name}] summary.{k}: rust {gv} vs oracle {wv}"));
        }
    }

    // ── 2. The pinned divergences, asserted in BOTH directions. ──────────────
    for (partition, table) in EXPECTED_DIVERGENCES {
        let v4_rows = want[partition][table].as_array().map(Vec::len).unwrap_or(0);
        let v5_rows = got
            .get(*partition)
            .and_then(|p| p.get(*table))
            .map(Vec::len)
            .unwrap_or(0);
        if v4_rows != 0 {
            failures.push(format!(
                "[{name}] {partition}.{table}: v4 restored {v4_rows} rows — the v4 bug this \
                 differential pins has been FIXED upstream; re-rule the divergence"
            ));
        }
        if v5_rows == 0 {
            failures.push(format!(
                "[{name}] {partition}.{table}: v5 restored NOTHING — the deliberate divergence \
                 (v5 restores what v4 cannot) has regressed"
            ));
        }
    }

    // ── 3. Every other table, row by row, after normalization. ───────────────
    let skip: HashSet<(&str, &str)> = EXPECTED_DIVERGENCES
        .iter()
        .chain(DIVERGENCE_DEPENDENTS.iter())
        .copied()
        .collect();

    // One normalizer per side, walked in the same order, so `<minted-N>` labels
    // correspond.
    let mut n_got = Normalizer::new(literals.clone());
    let mut n_want = Normalizer::new(literals);

    for (partition, tables) in got {
        for (table, rows) in tables {
            if skip.contains(&(partition.as_str(), table.as_str())) {
                continue;
            }
            let want_rows = want[partition][table].as_array().cloned().unwrap_or_default();
            let g = n_got.value(&Value::Array(rows.clone()));
            let wnt = n_want.value(&Value::Array(want_rows));
            if g != wnt {
                let ga = g.as_array().unwrap();
                let wa = wnt.as_array().unwrap();
                let detail = if ga.len() != wa.len() {
                    format!("row count: rust {} vs oracle {}", ga.len(), wa.len())
                } else {
                    ga.iter()
                        .zip(wa.iter())
                        .enumerate()
                        .find(|(_, (a, b))| a != b)
                        .map(|(i, (a, b))| {
                            format!("row {i}:\n    rust:   {a}\n    oracle: {b}")
                        })
                        .unwrap_or_default()
                };
                failures.push(format!("[{name}] {partition}.{table} differs\n  {detail}"));
            }
        }
    }
    if failures.is_empty() {
        println!(
            "OK {name}: {} tables diffed row-for-row across three partitions",
            got.values().map(|t| t.len()).sum::<usize>()
        );
    }
}
