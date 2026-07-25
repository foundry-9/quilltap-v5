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
use quilltap_core::services::backup::restore::{
    preview_restore, restore, RestoreMode, RestoreSummary,
};
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
/// Each entry is `(partition, table)`; those tables are excluded from the row
/// diff and checked by [`assert_divergences`] instead.
const EXPECTED_DIVERGENCES: &[(&str, &str)] = &[
    ("mountIndex", "doc_mount_points"),
    ("mountIndex", "doc_mount_file_links"),
    ("main", "files"),
];

/// ⚠ NOT a ruled divergence — an unfixed v5 GAP, recorded so the diff is honest.
///
/// v4 writes one `doc_mount_chunks` row per vault document as character
/// provisioning creates it (8 documents → 8 chunks in `restore_minimal`, whose
/// archive carries no mount data at all, so every one of them is provisioning's
/// work). v5 writes none: it creates chunks only through the explicit
/// reindex/embed paths, so a freshly provisioned vault is not semantically
/// searchable until something reindexes it.
///
/// That is **not restore's doing** — it is `create_character`'s vault write path,
/// which every character creation goes through, and it is invisible to the
/// characters family's own differentials because none of them dump this table. It
/// is therefore out of this unit's scope to fix, and wrong to normalize away
/// silently. The count is asserted as a KNOWN gap: v4 > 0 and v5 == 0. **When the
/// gap is closed this assertion fails**, which is the point — it is a tripwire,
/// not an excuse. Follow-up recorded in the lane record.
const KNOWN_V5_GAPS: &[(&str, &str)] = &[("mountIndex", "doc_mount_chunks")];

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
                        // An integral REAL dumps as `2` in JS and `2.0` here, which
                        // is a dump artifact rather than a data difference — SQLite
                        // has one NUMERIC affinity and both engines wrote the same
                        // value. Canonicalize to the integer so a REAL that is
                        // genuinely fractional still differs loudly.
                        ValueRef::Real(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
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
        && b.iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16 | 19 | 23) || c.is_ascii_digit())
}

/// Canonicalize a JSON-TEXT column: parse it, drop object keys whose value is
/// `null`, and re-emit. Applied to BOTH sides.
///
/// ## What this is for, and what it costs
///
/// v4 stores what Zod parsed, and a Zod `.nullable().optional()` field is
/// **omitted** when the input had no such key. v5 models those as `Option<T>`,
/// which serializes `None` as an explicit `null` — so v5's stored text carries
/// keys v4's does not. The live instance here is `chat_settings.cheapLLMSettings`
/// (`settings.types.ts:53,55,61` — `userDefinedProfileId`,
/// `defaultCheapProfileId`, `imagePromptProfileId` are all
/// `.nullable().optional()`); the archive omits them, v4 round-trips the omission,
/// v5 adds `null`.
///
/// That is a **pre-existing storage-fidelity gap in the chat-settings write path,
/// not restore's doing** — every settings write has it, and no prior differential
/// could see it because this is the first byte-level diff of that column
/// (`settings_routes_equivalence` compares parsed API bodies, where both sides
/// materialize the same defaults). Modelling it correctly needs
/// `Option<Option<String>>` (outer absent = key absent, `Some(None)` = explicit
/// `null`), which is the shape `chat_settings.rs` already documents for
/// `ThemePreference.custom_overrides` via `skip_serializing_if`. Doing that across
/// the settings bags ripples through every consumer, so it is a follow-up rather
/// than a restore lane's change. Recorded in the lane record.
///
/// **The cost, stated plainly:** this differential cannot see absent-vs-`null`
/// INSIDE a JSON column. It still sees every value difference, every added or
/// removed non-null key, and the whole rest of the row byte for byte.
fn canonical_json_text(s: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(s).ok()?;
    if !parsed.is_object() && !parsed.is_array() {
        return None;
    }
    fn strip(v: &Value) -> Value {
        match v {
            Value::Object(m) => Value::Object(
                m.iter()
                    .filter(|(_, val)| !val.is_null())
                    .map(|(k, val)| (k.clone(), strip(val)))
                    .collect(),
            ),
            Value::Array(a) => Value::Array(a.iter().map(strip).collect()),
            other => other.clone(),
        }
    }
    serde_json::to_string(&strip(&parsed)).ok()
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

    /// Replace every non-literal ISO timestamp appearing anywhere inside `s`.
    fn substitute_embedded_timestamps(&self, s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0usize;
        while i < b.len() {
            if i + 24 <= b.len() {
                if let Ok(window) = std::str::from_utf8(&b[i..i + 24]) {
                    if is_iso(window) && !self.literals.contains(window) {
                        out.push_str("<ts>");
                        i += 24;
                        continue;
                    }
                }
            }
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    fn value(&mut self, v: &Value) -> Value {
        match v {
            Value::String(s) => {
                if self.literals.contains(s) {
                    return v.clone();
                }
                // A JSON-text column: canonicalize, then normalize what is inside
                // it (a nested minted id or write-clock stamp still gets labelled).
                if (s.starts_with('{') || s.starts_with('[')) && s.len() > 1 {
                    if let Some(canon) = canonical_json_text(s) {
                        if let Ok(parsed) = serde_json::from_str::<Value>(&canon) {
                            let inner = self.value(&parsed);
                            return Value::String(serde_json::to_string(&inner).unwrap_or(canon));
                        }
                    }
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
                // A write-clock stamp EMBEDDED in a longer string. The live case is
                // a vault document's YAML front matter: folding a legacy outfit
                // preset into a wardrobe item stamps `createdAt`/`updatedAt` inside
                // the document body, so the string is not itself a timestamp but
                // contains two. Only stamps absent from the archive are replaced —
                // an archive timestamp that survives into a document body stays
                // under diff.
                if s.len() > 24 {
                    let sub = self.substitute_embedded_timestamps(s);
                    if sub != *s {
                        return Value::String(sub);
                    }
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
                let mut composite_normalized = false;
                for (k, val) in m {
                    let nv = self.value(val);
                    // Did a COMPOSITE string change under normalization? A bare id
                    // or timestamp column becoming `<minted-N>` / `<ts>` does not
                    // count — only a longer string that CONTAINS such a value: a
                    // JSON blob (a project's store document, whose
                    // `characterRoster` holds remapped ids) or a YAML body (a
                    // folded legacy wardrobe item, whose front matter holds write
                    // -clock stamps). Those are the only two shapes whose content
                    // hash cannot be reproduced across the two engines.
                    if let (Value::String(before), Value::String(after)) = (val, &nv) {
                        if before != after && !is_uuid(before) && !is_iso(before) {
                            composite_normalized = true;
                        }
                    }
                    out.insert(k.clone(), nv);
                }
                // A content hash over text that itself had to be normalized is
                // nondeterministic and holds no comparable information — but ONLY
                // then. Every other `*Sha256` in the dump (every document restored
                // verbatim from the archive, whose content is all archive literals
                // and so never changes here) stays under diff. The content itself
                // is still compared, normalized, immediately above.
                if composite_normalized {
                    for (k, val) in out.iter_mut() {
                        if k.to_ascii_lowercase().ends_with("sha256") && val.is_string() {
                            *val = Value::String("<sha:derived-from-normalized>".to_string());
                        }
                    }
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
        "restore_new_account" => "restore-archive.zip",
        other => panic!("unknown restore case {other}"),
    }
}

/// `new-account` is the only case that does NOT wipe first.
fn mode_for(name: &str) -> RestoreMode {
    match name {
        "restore_new_account" => RestoreMode::NewAccount,
        _ => RestoreMode::Replace,
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

        // The BASELINE, before a single restore write. See `compare_baseline`:
        // this is asserted against the oracle's own pre-restore dump FIRST, so a
        // provisioning difference is reported as a provisioning difference instead
        // of masquerading as a restore difference in every downstream table.
        drop(db);
        let got_pre = read_state(&instance);
        compare_baseline(name, &got_pre, &case["preState"], &mut failures);
        let db = reopen_instance(&instance);

        let summary = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(restore(&db, &host, &zip, mode_for(name), SINGLE_USER_ID))
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

    assert_eq!(seen, 4, "expected all four restore cases in the oracle");
    assert!(
        failures.is_empty(),
        "{} restore-state difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Reopen an already-provisioned instance (the baseline dump closes it first, so
/// every provisioning transaction has committed before it is read).
fn reopen_instance(dir: &Path) -> Db {
    Db::open(
        DbPaths {
            main: dir.join("quilltap.db"),
            mount_index: Some(dir.join("quilltap-mount-index.db")),
            llm_logs: Some(dir.join("quilltap-llm-logs.db")),
        },
        TEST_PEPPER,
    )
    .expect("reopen fresh instance")
}

/// The two sides' fresh instances must be identical BEFORE the restore runs, or
/// every post-state difference is ambiguous.
///
/// The previous lane hit exactly that ambiguity and read it the wrong way round:
/// it saw v4 finish `restore_minimal` with 8 `doc_mount_chunks` where v5 had 0
/// and diagnosed "a baseline or `delete_user_data` difference". The oracle's own
/// pre-restore dump settles it — **both baselines carry 0 chunks**, so those 8
/// rows are written BY the restore (v4 chunks each vault document as character
/// provisioning writes it) and the gap is a real behavioural difference, not
/// noise to subtract. Recorded as its own finding.
///
/// This is deliberately an assertion and not a subtraction: subtracting would
/// hide a provisioning drift that `provisioning_equivalence` does not cover
/// (it proves schema + the seed user / chat settings / embedding profile /
/// roleplay templates / the three built-in mounts — not everything a fresh
/// instance contains). Row COUNTS are compared, per table, because that is what
/// distinguishes "the instances started level" from "they did not"; the row
/// CONTENT of a fresh instance is `provisioning_equivalence`'s job, not this
/// file's.
fn compare_baseline(
    name: &str,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    for (partition, tables) in got {
        for (table, rows) in tables {
            let want_n = want
                .get(partition)
                .and_then(|p| p.get(table))
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            if rows.len() != want_n {
                failures.push(format!(
                    "[{name}] BASELINE {partition}.{table}: rust {} rows vs oracle {want_n} \
                     before any restore write — this is a PROVISIONING difference, not a \
                     restore one; fix it there (or extend provisioning_equivalence) rather \
                     than normalizing it away here",
                    rows.len()
                ));
            }
        }
    }
}

/// The order's `restore_preview_writes_nothing` arm — added at unification,
/// because no lane delivered it.
///
/// `system_restore_equivalence` diffs the preview's 41-key summary against v4's
/// and asserts the extract directory is cleaned up, and its header states that
/// `previewRestore` is "filesystem-only, touching no database". **That was an
/// assertion about the port, not a proof of it.** Nothing anywhere ran a preview
/// with a database in reach and checked the database afterwards — so a preview
/// that quietly wrote would have passed every test in the tree.
///
/// It matters more than it looks: preview is the one restore leg a user is
/// invited to run speculatively, on an instance full of data they have not agreed
/// to replace yet. "It only reads" has to be verified, not asserted.
///
/// This needs no oracle. It is an invariant of v5's own preview — v4's behaviour
/// is already pinned by the summary diff — so it runs unconditionally and can
/// never silently skip for a missing env var.
#[test]
fn preview_writes_nothing() {
    let scratch = fresh_scratch("preview-readonly");
    let instance = scratch.root.join("instance");
    let db = fresh_instance(&instance);
    // Restore a full archive first, so the preview runs against an instance with
    // real data in every table rather than a bare fresh one — a write that only
    // touched populated tables would slip past an empty instance.
    let host = TestHost {
        root: scratch.root.clone(),
    };
    std::fs::create_dir_all(host.temp_dir()).unwrap();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(restore(
            &db,
            &host,
            &archives_dir().join("restore-archive.zip"),
            RestoreMode::Replace,
            SINGLE_USER_ID,
        ))
        .expect("seed restore succeeded");
    drop(db);

    let before = read_state(&instance);
    let populated = before
        .values()
        .flat_map(|t| t.values())
        .filter(|rows| !rows.is_empty())
        .count();
    assert!(
        populated > 20,
        "the seed restore should leave a populated instance; only {populated} tables have rows"
    );

    // Every archive, including the two that throw.
    for archive in [
        "restore-archive.zip",
        "restore-archive-legacy.zip",
        "restore-archive-minimal.zip",
        "restore-archive-missing-required.zip",
        "restore-archive-malformed.zip",
    ] {
        let preview_root = scratch.root.join(format!("preview-{archive}"));
        std::fs::create_dir_all(&preview_root).unwrap();
        // The result is irrelevant here; `system_restore_equivalence` owns the
        // summary and the thrown messages. What matters is the database after.
        let _ = preview_restore(&archives_dir().join(archive), &preview_root);

        let after = read_state(&instance);
        assert_eq!(
            after, before,
            "previewing {archive} MUTATED the database — preview must be read-only"
        );
        assert!(
            is_empty(&preview_root),
            "previewing {archive} left its extract directory behind"
        );
    }
    println!("OK preview_writes_nothing: 5 archives previewed over {populated} populated tables, zero writes");
}

/// Is `dir` empty? (Local to this file; the equivalence family has its own.)
fn is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut e| e.next().is_none())
        .unwrap_or(true)
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

/// The ruled divergences, asserted in both directions — but only where the
/// ARCHIVE actually contains the thing.
///
/// The divergence is "v4 rejects rows it was given"; it says nothing about an
/// archive that carries no such rows. `restore_minimal` strips every optional
/// collection, so its post-restore mount points and file links are entirely
/// v4's freshly provisioned vaults, and asserting "v4 restored 0" there would
/// fail for a reason that has nothing to do with the bug.
///
/// `main.files` additionally needs somewhere to PUT the bytes: with no
/// `doc_mount_points` in the archive and the built-ins wiped, there is no store
/// to receive a project-less file in either engine, and both legitimately
/// restore zero. So the file divergence is asserted only when the archive
/// carries both the file rows and at least one mount point.
fn assert_divergences(
    name: &str,
    case: &Value,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    let archive = case["summary"].clone(); // preview/summary counts mirror the archive's input lengths
    let has = |k: &str| archive.get(k).and_then(Value::as_u64).unwrap_or(0) > 0;
    let n = |partition: &str, table: &str, side: &Value| {
        side.get(partition)
            .and_then(|p| p.get(table))
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    };
    let n_got = |partition: &str, table: &str| {
        got.get(partition)
            .and_then(|p| p.get(table))
            .map(Vec::len)
            .unwrap_or(0)
    };

    for (partition, table) in EXPECTED_DIVERGENCES {
        let applies = match *table {
            "doc_mount_points" => has("docMountPoints"),
            "doc_mount_file_links" => has("docMountFileLinks"),
            "files" => has("files") && has("docMountPoints"),
            other => panic!("unclassified divergence table {other}"),
        };
        if !applies {
            continue;
        }
        let v4 = n(partition, table, want);
        let v5 = n_got(partition, table);
        if v4 != 0 {
            failures.push(format!(
                "[{name}] {partition}.{table}: v4 restored {v4} rows from an archive that \
                 carries them — the v4 bug this differential pins has been FIXED upstream; \
                 re-rule the divergence"
            ));
        }
        if v5 == 0 {
            failures.push(format!(
                "[{name}] {partition}.{table}: v5 restored NOTHING from an archive that \
                 carries rows — the ruled divergence (v5 restores what v4 cannot) has regressed"
            ));
        }
    }

    // The chunk gap, asserted precisely rather than as "v5 has fewer".
    //
    // v5 restores the archive's chunk rows and nothing more; v4 restores those AND
    // writes one per vault document as provisioning creates it. So v5's table must
    // equal exactly what the restore reported restoring, and v4's must be that plus
    // its provisioning extras. Pinning both halves means the assertion fails if v5
    // ever starts chunking (the gap closed — good) OR if v5 stops restoring the
    // archive's chunks (a real regression the loose form would have hidden).
    for (partition, table) in KNOWN_V5_GAPS {
        let restored = archive
            .get("docMountChunks")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let v4 = n(partition, table, want);
        let v5 = n_got(partition, table);
        if v5 != restored {
            failures.push(format!(
                "[{name}] {partition}.{table}: v5 has {v5} rows but reported restoring \
                 {restored} — the archive's own chunks are no longer landing, which is a \
                 REGRESSION, not the recorded provisioning gap"
            ));
        }
        if v4 < v5 {
            failures.push(format!(
                "[{name}] {partition}.{table}: v4 {v4} < v5 {v5} — v4 is meant to write these \
                 rows AND more; re-examine the recorded gap"
            ));
        }
        if v4 == v5 {
            failures.push(format!(
                "[{name}] {partition}.{table}: v4 and v5 now agree at {v4} — the recorded v5 \
                 provisioning gap appears CLOSED. Remove the KNOWN_V5_GAPS entry and diff \
                 this table row for row."
            ));
        }
    }
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
    assert_divergences(name, case, got, want, failures);

    // ── 3. Every other table, row by row, after normalization. ───────────────
    let skip: HashSet<(&str, &str)> = EXPECTED_DIVERGENCES
        .iter()
        .chain(DIVERGENCE_DEPENDENTS.iter())
        .chain(KNOWN_V5_GAPS.iter())
        .copied()
        .collect();

    // A normalizer PER TABLE, per side.
    //
    // The banked draft used one normalizer for the whole walk so that a minted id
    // shared between two tables carried one label. That cannot work here, and the
    // reason is the divergence itself: v5 restores the archive's own mount points
    // and file links (real ids, so `literals`, so never labelled) where v4 rejects
    // them and mints fresh vault/store ids instead. v4 therefore mints ~30 more
    // ids than v5 over the same walk, and every global label after the first
    // divergent table is shifted — reporting one difference as fifty.
    //
    // Per-table labelling is stable under that. What it gives up, stated plainly:
    // a minted id is no longer proven to be the SAME minted id across two tables
    // (an `id` here and an FK there). The rows' shape, count, order and every
    // literal value are still compared exactly, and the FK's own table still
    // proves its target exists; only cross-table identity of minted ids is out of
    // scope. Regaining it needs a graph-level check, which is a bigger build than
    // this differential's claim requires.
    for (partition, tables) in got {
        for (table, rows) in tables {
            if skip.contains(&(partition.as_str(), table.as_str())) {
                continue;
            }
            let mut n_got = Normalizer::new(literals.clone());
            let mut n_want = Normalizer::new(literals.clone());
            let want_rows = want[partition][table]
                .as_array()
                .cloned()
                .unwrap_or_default();
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
                        .map(|(i, (a, b))| format!("row {i}:\n    rust:   {a}\n    oracle: {b}"))
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
