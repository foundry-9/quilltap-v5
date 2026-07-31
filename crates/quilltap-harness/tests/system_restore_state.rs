//! P4.9G5 restore differential, part 2 — the **tier-2 DB-state** diff for
//! `mode: 'replace'`.
//!
//! Both sides restore the *same committed archive* into a *freshly provisioned,
//! empty instance*, then every table in all three partitions is dumped row by
//! row and compared. A row-COUNT map would pass on a graph whose foreign keys
//! all point at nothing; the row dump would not.
//!
//! ## Normalization — mechanical, and origin-based throughout
//!
//! Two rules govern it; P4.d22 added two mechanical extensions of them, both
//! described at the end of this header.
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
//! ## The three ruled divergences — RETIRED (P4.d22, 2026-07-26)
//!
//! This file used to carry three. Running v4's REAL restore against v4's REAL
//! backup of a modern instance had shown that v4 could restore neither a
//! format-3/4 archive's document stores nor its user files, and a 2026-07-25
//! human ruling put v5 deliberately ahead of it. All three were pinned in BOTH
//! directions so an upstream fix could not pass unnoticed.
//!
//! **v4 fixed all three in `c1507f47`** (`fix(backup): restore brings back the
//! stores, the links, and the files`). The tripwires fired on the first
//! regenerated oracle, exactly as designed. What each one turned into:
//!
//! 1. **Mount points and file links** — v4 now coerces on the read side
//!    (`mount-index-coercion.ts`: JSON text → `string[]`, INTEGER 0/1 →
//!    `boolean`). CONVERGED, and byte-identically: the rows are diffed value for
//!    value here, not merely counted. Porting that coercion found a matching v5
//!    gap the count-level pin had hidden — v5 created the rows but with EMPTY
//!    pattern arrays, and would have read an INTEGER `0` policy flag as `true`.
//!    See `services::backup::restore::mount_index_coercion` and its own tier-1
//!    family, `backup_mount_index_coercion_equivalence`.
//! 2. **The `backupFormat === 2` gate** — now `>= 2` on both sides. CONVERGED.
//! 3. **The files phase's position** — v4 moved it from step 5 to `22a-bis`,
//!    after mount points and before folders/links. v5 runs it after the WHOLE
//!    doc-store family, and **RULED 2026-07-26 (human): v5 KEEPS its placement**
//!    — see [`PHASE_ORDER_RESIDUAL`]. A deliberate divergence, not a pending
//!    question.
//!
//! ## ⚠ What is left, and why each is not simply "fixed here"
//!
//! - [`PHASE_ORDER_RESIDUAL`] — the two orderings write the SAME ROWS with the
//!   SAME VALUES but in a different insertion order. **RULED: v5 keeps its
//!   placement** (2026-07-26). The residual stays asserted in both directions
//!   because the placements still differ — it now pins a DECISION, not an open
//!   question. **Do not "fix" v5 to v4's `22a-bis`.**
//! - [`V5_STATS_GAP`] — a pre-existing, separately-documented v5 deferral
//!   (`file_storage.rs`'s module header: v4's best-effort `refreshStats` is not
//!   ported) that only became visible once `main.files` came out of the
//!   divergence list. Not this lane's, and asserted in both directions.
//!
//! ## Two normalization rules this lane had to add
//!
//! Both were invisible while the divergent tables were skipped, and both are
//! pure nondeterminism rather than behaviour:
//!
//! - **Minted ids embedded anywhere in a string**, not just in a `/`-separated
//!   path. The live case is `mount-blob:<mountPointId>:<blobId>`, every restored
//!   file's `storageKey`. See `Normalizer::substitute_embedded_uuids`.
//! - **A content hash whose content had to be normalized, in a table with no
//!   content column beside it.** `doc_mount_documents` masks its own
//!   `contentSha256` when the document body carries write-clock stamps or
//!   remapped ids; the identical hash also sits in `doc_mount_files.sha256`,
//!   one table over, with nothing local to trigger the mask. See
//!   [`derived_shas`].
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

// The three v4-bug divergences this file used to carry
// (`("mountIndex","doc_mount_points")`, `("mountIndex","doc_mount_file_links")`,
// `("main","files")`) and the two tables their file phase made incomparable
// downstream (`doc_mount_blobs`, `doc_mount_files`) are **GONE** — v4 converged
// in `c1507f47` and every one of those tables is now diffed row for row. The
// earlier `KNOWN_V5_GAPS` chunk tripwire went the same way at P4.6BK. See the
// header. What remains is the two named residuals below.

/// ## ✅ RULED — v4's `22a-bis` vs v5's after-the-family
///
/// v4's `c1507f47` moved the files phase from step 5 to **`22a-bis`**: after
/// mount points (22a), before folders (22b) and links (22d). v5 runs it after
/// the WHOLE doc-store family (22a–22g). Both placements satisfy the real
/// dependency — the stores must exist before a bridge can resolve one — so both
/// restore the same file, into the same mount, at the same path.
///
/// **What the diff proves:** for these two tables the two sides hold the same
/// rows with the same values — an order-insensitive comparison passes, and every
/// other table (including `main.files` and `doc_mount_points`) matches row for
/// row in ORDER. What differs is only where the file phase's rows land in
/// **insertion order**: v4's restored blob and link sit at the 22a-bis position,
/// v5's at the end. No value differs. No row is missing.
///
/// **✅ RULED 2026-07-26 (human): v5 KEEPS its later placement. Do NOT adopt
/// v4's `22a-bis`.** The residual stays asserted in BOTH directions — the
/// multisets must match AND the raw orders must differ — so **aligning the two
/// placements fails this test**. That is now deliberate: the assertion pins a
/// decision rather than holding a question open.
///
/// **Why the later slot won, against the lane's own recommendation.** Both
/// placements carry a hazard, and neither is exercised by any committed
/// archive:
///
/// - v5's slot: after 22c the replay's `findOrCreateByContent` matches an
///   archived content row by sha and hard-links to it, so 22f's
///   `INSERT INTO doc_mount_blobs` violates `UNIQUE(fileId)` and the ARCHIVED
///   blob row is refused (v4 `found-bugs.md:361-370`).
/// - v4's slot: the replay wins the race into `restored/<name>`, so a
///   SECOND-GENERATION archive's own link rows collide with it and **the
///   archived link ids are lost** (v4 `found-bugs.md:385-397`, a residual v4
///   knowingly kept).
///
/// The ruling turns on what each slot makes POSSIBLE, not on which hazard is
/// milder. v4 names the proper repair itself and puts it out of scope: *"teach
/// the replay to recognise that the archive already carries the store rows for
/// a file and skip re-ingesting it, rather than reshuffling phase order"*
/// (`found-bugs.md:400-402`). That check can only be written from v5's slot —
/// at `22a-bis` the archived link and blob rows have not been restored yet, so
/// there is nothing to consult. v4 avoids the collision by arranging for
/// nothing to be there; v5's placement is the one where the file's identity can
/// actually be tested.
///
/// ## The re-examination P4.d23 owed (2026-07-26): the residual STAYS
///
/// The skip check landed (`carried_store_rows`, and [`REPLAY_DEDUPE`] for what
/// it costs v4), and both hazards are gone with it. The residual is **still
/// required**, and the reason is structural rather than incidental: the check
/// only fires for a file the archive carries store rows for. A file with a
/// LEGACY disk `storageKey` — which is exactly what `restore-archive.zip`'s one
/// `files` row has, and what every pre-mount-store instance's rows have — is
/// genuinely re-ingested on both sides, and the two slots still write its
/// content row, link and blob at different points in the insertion order. The
/// check removed the two HAZARDS; it did not, and could not, remove the ordering
/// difference that produced them. So these three tables keep their
/// order-insensitive comparison for as long as the two placements differ, which
/// the ruling says is permanently.
///
/// Each entry is `(case, partition, table)`.
/// The three tables the file phase writes into: a content row, a link row and
/// the blob bytes. `main.files` is NOT among them — the file phase is its only
/// writer, so there is nothing for its one row to be ordered against, and it is
/// diffed in order like everything else.
///
/// P4.D31 added the three `restore_memory_graph_new_account` entries. Same
/// reason as [`V5_STATS_GAP`]'s: that archive is `restore-archive.zip`'s
/// instance plus four memories and the same seeded `files/portrait.png`, so its
/// `new-account` restore replays one legacy-disk-key file and the two placements
/// still write its content row, link and blob at different points in the
/// insertion order. Not a new divergence — the ruled one, reached by a second
/// case of the same shape.
const PHASE_ORDER_RESIDUAL: &[(&str, &str, &str)] = &[
    ("restore_new_account", "mountIndex", "doc_mount_blobs"),
    ("restore_new_account", "mountIndex", "doc_mount_file_links"),
    ("restore_new_account", "mountIndex", "doc_mount_files"),
    (
        "restore_memory_graph_new_account",
        "mountIndex",
        "doc_mount_blobs",
    ),
    (
        "restore_memory_graph_new_account",
        "mountIndex",
        "doc_mount_file_links",
    ),
    (
        "restore_memory_graph_new_account",
        "mountIndex",
        "doc_mount_files",
    ),
];

/// ## A pre-existing v5 gap, newly VISIBLE — not this lane's to fix
///
/// v4's `storeMountFile` ends its database-blob branch with a best-effort
/// `repos.docMountPoints.refreshStats(mp.id)` (`store-file.ts:369`), which
/// recomputes the mount's cached `fileCount` / `chunkCount` / `totalSizeBytes`.
/// **v5 does not port it** — a deliberate, documented deferral with a standing
/// precedent across the groups / projects / image-generation paths
/// (`services/file_storage.rs` module header, `:31-34`).
///
/// So after a restore writes a user file through the uploads bridge, v4's
/// Quilltap Uploads mount reports `fileCount: 1, totalSizeBytes: 32` and v5's
/// still reports `0, 0`. The rows the counters summarize are identical on both
/// sides — the link, the content row and the blob all match byte for byte; only
/// the cached rollup is stale. It is user-visible (the Scriptorium's store cards
/// read these columns) and it is one call to fix, with v4's own values now in
/// hand as the oracle.
///
/// It is recorded rather than fixed because it belongs to `file_storage.rs`, not
/// to restore, and because a fix at this ONE call site would leave v5's other
/// bridge writes inconsistent with it. Asserted in both directions below, so
/// closing the deferral fails this test and forces the carve-out out.
///
/// Each entry is `(case, mount-point name)`; the three stat columns on that row
/// are masked and checked separately.
///
/// P4.D31 added `restore_memory_graph_new_account`. It is not a new gap: that
/// archive is `restore-archive.zip`'s instance plus four memories, seeded with
/// the same `files/portrait.png`, so its `new-account` restore replays the same
/// one user file into the target's own uploads mount and hits the same stale
/// rollup. The entry is the existing deferral reaching a second case of the same
/// shape, and it is asserted in both directions there too.
const V5_STATS_GAP: &[(&str, &str)] = &[
    ("restore_new_account", "Quilltap Uploads"),
    ("restore_memory_graph_new_account", "Quilltap Uploads"),
];

/// The columns [`V5_STATS_GAP`] makes incomparable on its named rows.
const STATS_COLUMNS: &[&str] = &["fileCount", "chunkCount", "totalSizeBytes"];

/// ## ⚠ THE RULED FILE-REPLAY DEDUPE DIVERGENCE (P4.d23, 2026-07-26)
///
/// v4 re-ingests every user file in the archive unconditionally. v5 does not:
/// when the archive already carries the document-store rows a file's
/// `storageKey` points at, the bytes are left where they are and only the
/// `files` row is written (`orchestrator.rs` → `carried_store_rows`).
///
/// **This divergence is deliberate and was ORDERED**, against a lane
/// recommendation to adopt v4's `22a-bis` phase order instead. v4 names this
/// repair itself and puts it out of scope (`found-bugs.md:400-402`); the human
/// ruling of 2026-07-26 kept v5's later files-phase slot precisely because the
/// check is only writable from there. `status-log.md` → "Ruling — the restore
/// file-replay dedupe". So this list GROWING is the ruling being implemented,
/// not P4.d22's convergence work regressing.
///
/// **What it costs v4, measured rather than reasoned.** On the second-generation
/// archive in `replace` mode v4's replay reaches `restored/<name>` first and its
/// own archived link rows are then refused:
///
/// ```text
/// Failed to restore doc-store file link "restored/portrait.png":
///   UNIQUE constraint failed: doc_mount_file_links.mountPointId, …relativePath
/// ```
///
/// The bytes survive (the replay wrote its own copy) but the archived link IDS
/// are lost, and every restore generation duplicates the store rows again. v5
/// restores that archive with zero warnings.
///
/// Asserted in BOTH directions by [`assert_replay_dedupe`]: reverting the check
/// fails these cases, and so does v4 adopting it.
const REPLAY_DEDUPE: &[&str] = &[
    "restore_uploads_replace",
    "restore_uploads_new_account",
    "restore_gen2_replace",
    "restore_gen2_new_account",
];

/// The tables [`REPLAY_DEDUPE`] makes incomparable row for row on its cases:
/// the `files` row whose storage key is preserved, and the four store tables v4
/// writes a second copy into.
const REPLAY_DEDUPE_TABLES: &[(&str, &str)] = &[
    ("main", "files"),
    ("mountIndex", "doc_mount_blobs"),
    ("mountIndex", "doc_mount_files"),
    ("mountIndex", "doc_mount_file_links"),
    ("mountIndex", "doc_mount_folders"),
];

/// The summary counters v4's OWN losses move: it restores fewer archived links
/// and folders than v5 because its replay got to their paths first.
const REPLAY_DEDUPE_SUMMARY_KEYS: &[&str] = &["docMountFolders", "docMountFileLinks"];

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

/// Every content hash, on ONE side, whose text is not reproducible across the
/// two engines — because that text carries a write-clock stamp or a minted id.
///
/// The `composite_normalized` rule inside [`Normalizer`] already masks such a
/// hash when the text sits in the same row: `doc_mount_documents` holds
/// `content` and `contentSha256` together, so a folded legacy wardrobe item
/// (write-clock stamps in its YAML front matter) or a project store document
/// (remapped ids in its `characterRoster`) masks its own hash.
///
/// **`doc_mount_files.sha256` is the same hash, one table over, with nothing
/// local to trigger the rule** — the row is just `{id, sha256, fileSizeBytes,
/// fileType, source}`. It was invisible while that table sat in the divergence
/// list; the moment it came out, two rows differed for pure nondeterminism.
///
/// So collect the hashes by ORIGIN, once per side, before the per-table walk,
/// and mask any `*sha256` VALUE that appears in the set. A hash over content
/// that normalizes to itself — every document restored verbatim from the
/// archive, which is nearly all of them — is untouched and stays under diff.
fn derived_shas(
    dump: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    literals: &HashSet<String>,
) -> HashSet<String> {
    let mut out = HashSet::new();
    for tables in dump.values() {
        for rows in tables.values() {
            for row in rows {
                let (Some(content), Some(Value::String(sha))) = (
                    row.get("content").and_then(Value::as_str),
                    row.get("contentSha256"),
                ) else {
                    continue;
                };
                if contains_nonliteral(content, literals) {
                    out.insert(sha.clone());
                }
            }
        }
    }
    out
}

/// Does `s` contain a UUID or ISO timestamp the archive does not vouch for?
fn contains_nonliteral(s: &str, literals: &HashSet<String>) -> bool {
    let b = s.as_bytes();
    for i in 0..b.len() {
        for len in [24usize, 36] {
            if i + len > b.len() {
                continue;
            }
            let Ok(w) = std::str::from_utf8(&b[i..i + len]) else {
                continue;
            };
            let shaped = if len == 24 { is_iso(w) } else { is_uuid(w) };
            if shaped && !literals.contains(w) {
                return true;
            }
        }
    }
    false
}

/// The normalization rules, applied over the whole dump in walk order.
struct Normalizer {
    literals: HashSet<String>,
    derived_shas: HashSet<String>,
    minted: BTreeMap<String, String>,
}

impl Normalizer {
    fn new(literals: HashSet<String>, derived_shas: HashSet<String>) -> Self {
        Normalizer {
            literals,
            derived_shas,
            minted: BTreeMap::new(),
        }
    }

    /// Replace every minted UUID appearing anywhere inside `s`, whatever the
    /// surrounding punctuation.
    ///
    /// This used to be a `'/'`-split, which covered a filesystem-shaped storage
    /// key and nothing else. The live counter-example is the mount-blob storage
    /// key `mount-blob:<mountPointId>:<blobId>` — **colon**-separated, two
    /// minted ids, and therefore never comparable across the two engines. It was
    /// invisible while `main.files` sat in `EXPECTED_DIVERGENCES`; the moment
    /// that came out, every restored file's `storageKey` differed for a reason
    /// that is pure nondeterminism. Scanning for the UUID shape itself is
    /// punctuation-agnostic and cannot go stale the next time a key format
    /// changes.
    fn substitute_embedded_uuids(&mut self, s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0usize;
        while i < b.len() {
            if i + 36 <= b.len() {
                if let Ok(window) = std::str::from_utf8(&b[i..i + 36]) {
                    if is_uuid(window) && !self.literals.contains(window) {
                        let next = self.minted.len();
                        let label = self
                            .minted
                            .entry(window.to_string())
                            .or_insert_with(|| format!("<minted-{next}>"));
                        out.push_str(label);
                        i += 36;
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
                // A storage key embeds one or two minted ids (`mount-blob:<mp>:
                // <blob>`, or a slash-shaped path). Normalize them wherever they
                // sit — see `substitute_embedded_uuids`.
                if s.len() > 36 {
                    let sub = self.substitute_embedded_uuids(s);
                    if sub != *s {
                        return Value::String(sub);
                    }
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
                for (k, val) in out.iter_mut() {
                    if !k.to_ascii_lowercase().ends_with("sha256") || !val.is_string() {
                        continue;
                    }
                    // …either because the text it hashes is right here and had to
                    // be normalized…
                    if composite_normalized
                        // …or because it is the SAME hash, one table over, with
                        // no content column beside it to trigger the rule. See
                        // `derived_shas`.
                        || val
                            .as_str()
                            .is_some_and(|s| self.derived_shas.contains(s))
                    {
                        *val = Value::String("<sha:derived-from-normalized>".to_string());
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
        "restore_uploads_replace" | "restore_uploads_new_account" => "restore-archive-uploads.zip",
        "restore_gen2_replace" | "restore_gen2_new_account" => "restore-archive-gen2.zip",
        "restore_memory_graph_replace" | "restore_memory_graph_new_account" => {
            "restore-archive-memory-graph.zip"
        }
        other => panic!("unknown restore case {other}"),
    }
}

/// `new-account` is the only mode that does NOT wipe first.
fn mode_for(name: &str) -> RestoreMode {
    if name.ends_with("_new_account") {
        RestoreMode::NewAccount
    } else {
        RestoreMode::Replace
    }
}

/// P4.d23: does this case repoint the fresh target at the ARCHIVE's own uploads
/// mount before restoring?
///
/// Without it no `replace` case reaches the file replay at all. `deleteUserData`
/// drops every `doc_mount_points` row but deliberately leaves `instance_settings`
/// alone, so the surviving `userUploadsMountPointId` names the target's own
/// (now-deleted) mount, 22a restores the ARCHIVE's mounts under the archive's
/// ids, and the pointer dangles — both engines warn and restore nothing. Setting
/// it is what disaster recovery IS: you are restoring your own backup onto your
/// own instance, where those ids agree.
///
/// It is setup, not normalization: both sides read the value out of the archive
/// and write it before the baseline is dumped, so the two baselines still
/// describe the same instance and `compare_baseline` still means what it says.
///
/// Deliberately NOT applied in `new-account` mode — nothing is wiped and every
/// archive id is remapped, so an aligned pointer would name a mount that no
/// longer exists. There the replay correctly lands in the target's OWN uploads
/// mount, which still shares the archive's CONTENT rows because `doc_mount_files`
/// is global and keyed by sha.
fn aligns_uploads_pointer(name: &str) -> bool {
    matches!(name, "restore_uploads_replace" | "restore_gen2_replace")
}

/// ## P4.D31 — the memory-id contract, asserted on v5 ALONE
///
/// The row-for-row diff above already says "v5 restores the memories v4
/// restores". This says something the diff cannot: that the restored graph is
/// **internally closed**. A diff is an agreement test — if both engines minted
/// fresh ids the same way, it would pass while every `relatedMemoryIds` edge
/// pointed at nothing. That is exactly the failure v4 shipped for as long as it
/// did (`4ac66c29`), and the failure v5 inherited by porting it faithfully, so
/// the standing check is worth its few lines.
///
/// Three claims, per case, over the ARCHIVE's own memories:
///
/// 1. **Count.** Every archived memory came back (this is a fresh instance, so
///    the restored set IS the archive's set in both modes).
/// 2. **Identity.** In `replace` mode each row lands under its ARCHIVED id,
///    verbatim. In `new-account` mode `remap_backup_data` runs first, so the
///    archived ids must all be GONE and replaced by a bijective relabel —
///    asserted as "same cardinality, disjoint from the archive's ids".
/// 3. **Closure.** Every `relatedMemoryIds` edge resolves to a restored row, and
///    the edge COUNT per row matches the archive's. A fresh mint breaks (3) in
///    both modes, which is what makes this the mode-independent detector; (2) is
///    what names *why*.
fn assert_memory_graph_intact(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    failures: &mut Vec<String>,
) {
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its memories");
    let archived = &extracted.data.memories;
    let empty: Vec<Value> = Vec::new();
    let rows = got
        .get("main")
        .and_then(|p| p.get("memories"))
        .unwrap_or(&empty);

    // 1. count
    if rows.len() != archived.len() {
        failures.push(format!(
            "[{name}] MEMORY GRAPH: restored {} memories, archive carries {}",
            rows.len(),
            archived.len()
        ));
        return;
    }

    let archived_ids: HashSet<&str> = archived
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .collect();
    let restored_ids: HashSet<&str> = rows
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str))
        .collect();

    // 2. identity
    if name.ends_with("_new_account") {
        let overlap: Vec<&str> = restored_ids.intersection(&archived_ids).copied().collect();
        if !overlap.is_empty() {
            failures.push(format!(
                "[{name}] MEMORY GRAPH: new-account restore kept archived memory id(s) \
                 {overlap:?} — remap_backup_data must relabel every one"
            ));
        }
        if restored_ids.len() != archived_ids.len() {
            failures.push(format!(
                "[{name}] MEMORY GRAPH: {} distinct restored ids for {} archived — the \
                 new-account relabel must be a bijection",
                restored_ids.len(),
                archived_ids.len()
            ));
        }
    } else if restored_ids != archived_ids {
        let missing: Vec<&str> = archived_ids.difference(&restored_ids).copied().collect();
        let extra: Vec<&str> = restored_ids.difference(&archived_ids).copied().collect();
        failures.push(format!(
            "[{name}] MEMORY GRAPH: replace restore did not preserve archived memory ids \
             (missing {missing:?}, unexpected {extra:?}) — v4 `restore.ts:189` passes `{{ id }}`"
        ));
    }

    // 3. closure. `relatedMemoryIds` is a TEXT column holding a JSON array.
    let edges_of = |m: &Value| -> Vec<String> {
        match m.get("relatedMemoryIds") {
            Some(Value::String(s)) => serde_json::from_str::<Vec<String>>(s).unwrap_or_default(),
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    };
    let archived_edges: usize = archived.iter().map(|m| edges_of(m).len()).sum();
    let restored_edges: usize = rows.iter().map(|m| edges_of(m).len()).sum();
    if archived_edges != restored_edges {
        failures.push(format!(
            "[{name}] MEMORY GRAPH: {restored_edges} relatedMemoryIds edges restored, \
             archive carries {archived_edges}"
        ));
    }
    for m in rows {
        let id = m.get("id").and_then(Value::as_str).unwrap_or("<no id>");
        for edge in edges_of(m) {
            if !restored_ids.contains(edge.as_str()) {
                failures.push(format!(
                    "[{name}] MEMORY GRAPH: memory {id} points at {edge}, which no restored \
                     memory carries — the Commonplace Book's graph came back flattened"
                ));
            }
        }
    }
}

/// Repoint `instance_settings.userUploadsMountPointId` at the archive's own
/// uploads mount — the raw upsert v4's provisioning migration uses (a bare id,
/// NOT JSON-encoded).
fn align_uploads_pointer(instance: &Path, zip: &Path, temp_root: &Path) {
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its uploads pointer");
    let value = extracted
        .data
        .instance_settings
        .iter()
        .find(|r| r.get("key").and_then(Value::as_str) == Some("userUploadsMountPointId"))
        .and_then(|r| r.get("value").and_then(Value::as_str))
        .unwrap_or_else(|| panic!("{zip:?} carries no userUploadsMountPointId to align to"))
        .to_string();
    let conn = quilltap_core::db::Writer::open_writable(&instance.join("quilltap.db"), TEST_PEPPER)
        .expect("open main to align the uploads pointer");
    conn.connection()
        .execute(
            "INSERT INTO \"instance_settings\" (\"key\", \"value\") VALUES (?1, ?2) \
             ON CONFLICT(\"key\") DO UPDATE SET \"value\" = excluded.\"value\"",
            rusqlite::params!["userUploadsMountPointId", value],
        )
        .expect("align the uploads pointer");
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

// (`DIVERGENT_SUMMARY_KEYS` is gone with the divergences: `files`,
// `docMountPoints` and `docMountFileLinks` are now compared like every other
// summary counter.)

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
        if aligns_uploads_pointer(name) {
            align_uploads_pointer(&instance, &zip, &host.temp_dir());
        }
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

        // v5 alone: the restored memory graph must be internally closed. Run for
        // EVERY case, not just the memory-graph ones — the other archives' edge
        // sets are empty, so it is cheap there and guards the id preservation.
        assert_memory_graph_intact(name, &zip, &host.temp_dir(), &got_state, &mut failures);

        let literals = archive_literals(&zip, &host.temp_dir());
        compare_case(
            name,
            &summary,
            case,
            &got_state,
            want_state,
            literals,
            &zip,
            &host.temp_dir(),
            &mut failures,
        );
    }

    assert_eq!(seen, 10, "expected all ten restore cases in the oracle");
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

/// `summary.warnings` — put under diff by P4.d22, having never been compared.
///
/// It matters most on exactly the phase this round converged: a per-file failure
/// leaves a warning and nothing else, so "both engines restored zero files" is
/// only meaningful alongside "…and said the same thing about why". On the three
/// `replace`-mode archives both engines now emit, for the same file, the same
/// sentence — that is the strongest single statement this differential makes
/// about bug 3.
///
/// ## The one masked substring, and the v5 bug behind it
///
/// v5's warning reads `Failed to restore file "portrait.png": key derivation
/// failed: Quilltap Uploads mount has not been provisioned` where v4's reads
/// `…: Quilltap Uploads mount has not been provisioned`. The extra clause is not
/// a different failure — it is `DbError::Key`'s Display prefix leaking into
/// user-visible text. `DbError::Key`'s doc comment says "Deriving the raw-hex key
/// from the pepper failed", but roughly twenty call sites across the crate use it
/// as a general-purpose message carrier (`tools/state.rs`, `tools/help.rs`,
/// `enclave/lifecycle.rs`, the two file-storage bridges, …), so every one of them
/// prints a cipher-flavoured lie in front of its real message.
///
/// That is a pre-existing, crate-wide naming problem, **not restore's and not
/// this lane's** — fixing it means either a new additive variant or a Display
/// change that moves strings other differentials pin. Recorded in the lane
/// record; the prefix is stripped here so the rest of the sentence is compared
/// verbatim. Once the prefix is gone the strip is simply a no-op, so a future fix
/// needs no change here.
fn compare_warnings(name: &str, got: &Value, want: &Value, failures: &mut Vec<String>) {
    const LEAKED_PREFIX: &str = "key derivation failed: ";
    let strip = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.replace(LEAKED_PREFIX, ""))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (g, w) = (strip(got), strip(want));
    if g != w {
        failures.push(format!(
            "[{name}] summary.warnings differ\n  rust:   {g:?}\n  oracle: {w:?}"
        ));
    }
}

/// [`V5_STATS_GAP`], asserted in BOTH directions: on the named mount-point row
/// v4's `refreshStats` must have produced a non-zero `fileCount` and v5's
/// unported one must still read zero. **Close the deferral and this fails**, at
/// which point the mask below comes out and the columns go back under diff.
fn assert_stats_gap(
    name: &str,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    let count_for = |rows: &[Value], mount: &str| -> Option<f64> {
        rows.iter()
            .find(|r| r.get("name").and_then(Value::as_str) == Some(mount))
            .and_then(|r| r.get("fileCount"))
            .and_then(Value::as_f64)
    };
    for (case, mount) in V5_STATS_GAP {
        if *case != name {
            continue;
        }
        let v5 = got
            .get("mountIndex")
            .and_then(|p| p.get("doc_mount_points"))
            .and_then(|rows| count_for(rows, mount));
        let v4 = want
            .get("mountIndex")
            .and_then(|p| p.get("doc_mount_points"))
            .and_then(Value::as_array)
            .and_then(|rows| count_for(rows, mount));
        match (v5, v4) {
            (Some(v5), Some(v4)) => {
                if v4 == 0.0 {
                    failures.push(format!(
                        "[{name}] {mount}.fileCount: v4 reports 0 — it is supposed to \
                         refreshStats after the bridge write; the gap this masks may have \
                         moved, so re-check it rather than widening the mask"
                    ));
                }
                if v5 != 0.0 {
                    failures.push(format!(
                        "[{name}] {mount}.fileCount: v5 reports {v5}, so the unported \
                         `refreshStats` deferral (file_storage.rs module header) has been \
                         CLOSED — delete this row from V5_STATS_GAP and let the three stat \
                         columns go back under diff"
                    ));
                }
            }
            _ => failures.push(format!(
                "[{name}] V5_STATS_GAP names a mount point `{mount}` that one side does not \
                 have — the carve-out has gone stale"
            )),
        }
    }
}

/// The archive's own "carried" files: rows whose `storageKey` names a
/// document-store blob the archive also ships. Exactly the set
/// `carried_store_rows` will short-circuit — computed here from the archive
/// alone, so the test does not take the port's word for it.
///
/// Returns `(originalFilename, archive storage key)` pairs.
fn carried_files(zip: &Path, temp_root: &Path) -> Vec<(String, String)> {
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its carried files");
    let blob_ids: HashSet<String> = extracted
        .data
        .doc_mount_blobs
        .iter()
        .filter_map(|b| b.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    extracted
        .data
        .files
        .iter()
        .filter_map(|f| {
            let key = f.get("storageKey").and_then(Value::as_str)?;
            let (_, blob) =
                quilltap_core::services::file_storage::parse_mount_blob_storage_key(key)?;
            blob_ids.contains(&blob).then(|| {
                (
                    f.get("originalFilename")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    key.to_string(),
                )
            })
        })
        .collect()
}

/// One side's `main.files` storage keys, by `originalFilename`.
fn storage_keys_by_name(files: &[Value]) -> BTreeMap<String, String> {
    files
        .iter()
        .filter_map(|f| {
            Some((
                f.get("originalFilename")
                    .and_then(Value::as_str)?
                    .to_string(),
                f.get("storageKey")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ))
        })
        .collect()
}

/// [`REPLAY_DEDUPE`], asserted in BOTH directions.
///
/// Four claims, all mechanical from the archive plus the two post-states. Nothing
/// here is a subtraction: the divergent tables are not diffed, but every property
/// that makes the divergence the RIGHT one is checked.
///
/// **A. v5 kept the archive's store rows, and the file still resolves.** For each
/// carried file v5's restored `storageKey` names a blob that is actually present
/// in v5's `doc_mount_blobs` — the failure this surface fears most is silently
/// dropping a file the user expected back — and v5 emitted no UNIQUE-constraint
/// warning at all.
///
/// **B. v4 re-ingested, once per carried file.** v4 holds exactly `carried` more
/// `doc_mount_blobs` and `doc_mount_files` rows than v5. This is a statement
/// about **v4's** slot, not a tripwire on v5's check: at `22a-bis` v4's replay
/// runs before 22c/22f, finds no content row to match on, and mints its own.
///
/// **C. The keys differ.** v4's `files` row points at the blob its replay minted;
/// v5's points at the archive's own.
///
/// **D. The second-generation loss, pinned by its wording.** On the gen-2 archive
/// in `replace` mode v4 must report a refused archived link for each carried
/// file, and v5 must report none. That is the data loss the ruling declined to
/// reproduce, and it is the whole reason the check exists.
///
/// **E. The link arithmetic — THE tripwire.** This is the one that fails if the
/// skip check is reverted, and it took a mutation test to find out that (B) and
/// (C) do not.
///
/// v5's slot turns out NOT to carry the `UNIQUE(fileId)` hazard v4 predicted for
/// it (`found-bugs.md:361-370`), and the reason is worth keeping: v5's
/// `link_blob_content` **upserts** the blob by `fileId` rather than inserting, so
/// a replay that matches the archived content row by sha REUSES the archived blob
/// instead of colliding with it. No violation, no refused row — and therefore no
/// visible damage in the blob or content tables at all.
///
/// What a v5 re-ingest actually costs is one spurious **link**: a second path
/// pointing at bytes the archive already had, unique-suffixed to
/// `restored/<name> (2)` when the archive's own link is already sitting there —
/// which is how a second-generation archive accumulates another copy on every
/// restore. So the arithmetic is the measurement:
///
/// ```text
/// v4_links − v5_links == carried − (archived links v4 refused)
/// ```
///
/// Both sides create one link per genuinely re-ingested file, so those cancel;
/// what is left is v4's replay link per carried file, less the archived links its
/// replay then locked out. Re-ingesting on the v5 side moves the left-hand side
/// by exactly `carried` and this fails.
#[allow(clippy::too_many_arguments)]
fn assert_replay_dedupe(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    summary: &RestoreSummary,
    want_summary: &Value,
    failures: &mut Vec<String>,
) {
    let carried = carried_files(zip, temp_root);
    if carried.is_empty() {
        failures.push(format!(
            "[{name}] REPLAY_DEDUPE names this case but its archive carries no \
             store-backed file — the carve-out has gone stale"
        ));
        return;
    }

    let empty: Vec<Value> = Vec::new();
    let got_files = got
        .get("main")
        .and_then(|p| p.get("files"))
        .unwrap_or(&empty);
    let want_files: Vec<Value> = want["main"]["files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let got_keys = storage_keys_by_name(got_files);
    let want_keys = storage_keys_by_name(&want_files);
    let got_blob_ids: HashSet<String> = got
        .get("mountIndex")
        .and_then(|p| p.get("doc_mount_blobs"))
        .unwrap_or(&empty)
        .iter()
        .filter_map(|b| b.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();

    // ── A + C, per carried file ──────────────────────────────────────────────
    for (filename, archive_key) in &carried {
        let Some(v5) = got_keys.get(filename) else {
            failures.push(format!(
                "[{name}] v5 restored no `files` row for the carried file {filename:?} — the \
                 skip check must never drop a file"
            ));
            continue;
        };
        match quilltap_core::services::file_storage::parse_mount_blob_storage_key(v5) {
            Some((_, blob)) if got_blob_ids.contains(&blob) => {}
            _ => failures.push(format!(
                "[{name}] v5's storageKey for {filename:?} is {v5:?}, which names no blob \
                 present in the restored store (the archive's key was {archive_key:?}) — the \
                 skip check has left the file unreachable"
            )),
        }
        if let Some(v4) = want_keys.get(filename) {
            if v4 == v5 {
                failures.push(format!(
                    "[{name}] v4 and v5 now agree on {filename:?}'s storageKey ({v5:?}) — either \
                     the ruled skip check was reverted or v4 adopted it. Re-rule the divergence \
                     and take this case out of REPLAY_DEDUPE rather than widening the carve-out."
                ));
            }
        } else {
            failures.push(format!(
                "[{name}] v4 restored no `files` row for {filename:?} — the carve-out is \
                 describing a case that no longer exists"
            ));
        }
    }

    // ── B, the duplicate store rows ──────────────────────────────────────────
    for table in ["doc_mount_blobs", "doc_mount_files"] {
        let g = got
            .get("mountIndex")
            .and_then(|p| p.get(table))
            .map(Vec::len)
            .unwrap_or(0) as i64;
        let wn = want["mountIndex"][table]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0) as i64;
        if wn - g != carried.len() as i64 {
            failures.push(format!(
                "[{name}] mountIndex.{table}: v4 has {wn} rows and v5 has {g}, a difference of \
                 {} where the {} carried file(s) predict {}. v4 re-ingests exactly once per \
                 carried file; anything else means the divergence has changed shape and must be \
                 re-examined, not re-fitted.",
                wn - g,
                carried.len(),
                carried.len()
            ));
        }
    }

    // ── D, the second-generation loss ────────────────────────────────────────
    const REFUSED_LINK: &str = "Failed to restore doc-store file link";
    let count_refused = |v: &[String]| v.iter().filter(|s| s.starts_with(REFUSED_LINK)).count();
    let got_warn: Vec<String> = summary.warnings.clone();
    let want_warn: Vec<String> = want_summary["warnings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if count_refused(&got_warn) != 0 {
        failures.push(format!(
            "[{name}] v5 refused an archived link of its own: {got_warn:?} — the skip check is \
             supposed to leave the archive's store rows unmolested"
        ));
    }
    // ── E, the link arithmetic — the tripwire ────────────────────────────────
    let refused_by_v4 = count_refused(&want_warn);
    let link_rows = |rows: Option<&Vec<Value>>| rows.map(Vec::len).unwrap_or(0) as i64;
    let g_links = link_rows(
        got.get("mountIndex")
            .and_then(|p| p.get("doc_mount_file_links")),
    );
    let w_links = want["mountIndex"]["doc_mount_file_links"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0) as i64;
    let expected = carried.len() as i64 - refused_by_v4 as i64;
    if w_links - g_links != expected {
        failures.push(format!(
            "[{name}] mountIndex.doc_mount_file_links: v4 has {w_links} and v5 has {g_links}, a \
             difference of {} where {} carried file(s) less {refused_by_v4} archived link(s) v4 \
             refused predict {expected}. v5 must create NO replay link for a carried file — if \
             it has started to, the ruled skip check has been reverted or narrowed; if v4 has \
             stopped, it has adopted the check and the divergence should be retired.",
            w_links - g_links,
            carried.len()
        ));
    }

    if name == "restore_gen2_replace" {
        let refused = count_refused(&want_warn);
        if refused != carried.len() {
            failures.push(format!(
                "[{name}] v4 refused {refused} archived link(s) where the {} carried file(s) \
                 predict {}. This case exists to pin v4's second-generation data loss; if v4 no \
                 longer loses those links, the ruling's premise has moved and the divergence \
                 must be re-ruled.\n  v4 warnings: {want_warn:?}",
                carried.len(),
                carried.len()
            ));
        }
    }

    // ── The uploads mount's rollups, masked above, bounded here ──────────────
    //
    // v4 re-ingests and then `refreshStats`; v5 leaves the archive's own rollup
    // exactly as the backup recorded it. Summed across every `Quilltap Uploads`
    // row so it reads the same in both modes (in `new-account` the target keeps
    // its own uploads mount beside the archive's remapped one).
    //
    // This is a BOUND, not the tripwire — v4 can never end up counting fewer
    // files than v5 here, but the two can legitimately coincide: on the
    // second-generation archive the rollup the backup recorded and the one v4
    // recomputes after re-ingesting happen to be the same number. (B) above is
    // what actually fails if the skip check stops firing.
    let uploads_file_count = |rows: &[Value]| -> f64 {
        rows.iter()
            .filter(|r| r.get("name").and_then(Value::as_str) == Some("Quilltap Uploads"))
            .filter_map(|r| r.get("fileCount").and_then(Value::as_f64))
            .sum()
    };
    let g_stats = uploads_file_count(
        got.get("mountIndex")
            .and_then(|p| p.get("doc_mount_points"))
            .unwrap_or(&empty),
    );
    let w_stats = uploads_file_count(
        &want["mountIndex"]["doc_mount_points"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );
    if g_stats > w_stats {
        failures.push(format!(
            "[{name}] Quilltap Uploads fileCount: v5 totals {g_stats} and v4 totals {w_stats}. \
             v4 re-ingests every carried file and then recomputes the rollup, so it can never \
             count FEWER than v5 — this is a real difference, not the masked one."
        ));
    }

    // ── The two summary counters those losses move ───────────────────────────
    for key in REPLAY_DEDUPE_SUMMARY_KEYS {
        let g = serde_json::to_value(summary).expect("summary serializes")[key]
            .as_i64()
            .unwrap_or(0);
        let wn = want_summary[key].as_i64().unwrap_or(0);
        if g < wn {
            failures.push(format!(
                "[{name}] summary.{key}: v5 restored {g} where v4 restored {wn}. v5 must restore \
                 at least as many archived rows as v4 — v4's shortfall is its replay refusing \
                 them, and v5 has no replay to refuse anything."
            ));
        }
    }
}

/// Normalize `rows` under a CANONICAL row order rather than the insertion one.
///
/// The insertion-order normalizer cannot be reused directly for an
/// order-insensitive comparison, and the reason is the labelling: `<minted-N>`
/// is assigned in first-encounter order, so moving one row renumbers every label
/// after it. Sorting the already-labelled rows would compare two different
/// labellings of the same data.
///
/// So: label once to get a stable, engine-independent sort key (with the label
/// NUMBERS collapsed, since those are what the reordering perturbs), sort the
/// RAW rows by it, then label again from scratch over the canonical order. The
/// result is a full-fidelity normalization — minted-id identity WITHIN the table
/// is still proven — under an order both engines agree on.
fn normalize_canonically(
    rows: &[Value],
    literals: &HashSet<String>,
    shas: &HashSet<String>,
) -> Vec<Value> {
    let mut keyed: Vec<(String, Value)> = rows
        .iter()
        .map(|row| {
            let mut n = Normalizer::new(literals.clone(), shas.clone());
            let labelled = n.value(row).to_string();
            let key = collapse_labels(&labelled);
            (key, row.clone())
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    let mut n = Normalizer::new(literals.clone(), shas.clone());
    keyed.into_iter().map(|(_, row)| n.value(&row)).collect()
}

/// `<minted-7>` → `<minted>`; the label's NUMBER is exactly what a reordering
/// perturbs, so it cannot be part of a canonical sort key.
fn collapse_labels(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("<minted-") {
        out.push_str(&rest[..i]);
        out.push_str("<minted>");
        rest = match rest[i..].find('>') {
            Some(j) => &rest[i + j + 1..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// [`PHASE_ORDER_RESIDUAL`], asserted in BOTH directions: the two sides must
/// hold the same rows (under a canonical order) AND their raw insertion orders
/// must differ. **Align the two phase orders and this fails**, forcing the
/// carve-out out.
fn assert_phase_order_residual(
    name: &str,
    table: &str,
    got: &[Value],
    want: &[Value],
    literals: &HashSet<String>,
    shas_got: &HashSet<String>,
    shas_want: &HashSet<String>,
) -> Vec<String> {
    let mut failures = Vec::new();
    let cg = normalize_canonically(got, literals, shas_got);
    let cw = normalize_canonically(want, literals, shas_want);
    if cg != cw {
        let detail = if cg.len() != cw.len() {
            format!("row count: rust {} vs oracle {}", cg.len(), cw.len())
        } else {
            cg.iter()
                .zip(cw.iter())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| format!("canonical row {i}:\n    rust:   {a}\n    oracle: {b}"))
                .unwrap_or_default()
        };
        failures.push(format!(
            "[{name}] mountIndex.{table}: the ROWS differ, not just their order — this is NOT \
             the documented phase-order residual, and the residual must not be used to absorb \
             it\n  {detail}"
        ));
        return failures;
    }
    // Same rows. Now prove the residual is still real: the insertion orders must
    // still differ, or this carve-out is describing something that no longer
    // exists.
    let mut n_got = Normalizer::new(literals.clone(), shas_got.clone());
    let mut n_want = Normalizer::new(literals.clone(), shas_want.clone());
    let ordered_got = collapse_labels(&n_got.value(&Value::Array(got.to_vec())).to_string());
    let ordered_want = collapse_labels(&n_want.value(&Value::Array(want.to_vec())).to_string());
    if ordered_got == ordered_want {
        failures.push(format!(
            "[{name}] mountIndex.{table}: the insertion orders now MATCH — the phase-order \
             residual is gone (v5's file phase moved, or v4's did). The 2026-07-26 ruling says \
             v5 KEEPS its later slot, so this is a divergence from the ruling and not merely a \
             stale carve-out: re-rule it before removing the entry."
        ));
    } else {
        println!(
            "  residual {name}/{table}: same rows, different insertion order \
             (the RULED placement divergence)"
        );
    }
    failures
}

#[allow(clippy::too_many_arguments)]
fn compare_case(
    name: &str,
    summary: &RestoreSummary,
    case: &Value,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    literals: HashSet<String>,
    zip: &Path,
    temp_root: &Path,
    failures: &mut Vec<String>,
) {
    // ── 1. Every summary counter except `warnings`. ──────────────────────────
    //
    // `files`, `docMountPoints` and `docMountFileLinks` used to be excluded here
    // as divergent. They are compared like the rest now.
    let dedupe = REPLAY_DEDUPE.contains(&name);
    let got_summary = serde_json::to_value(summary).expect("summary serializes");
    let want_summary = &case["summary"];
    for (k, gv) in got_summary.as_object().unwrap() {
        let wv = &want_summary[k];
        if k == "warnings" {
            // The dedupe cases' warnings ARE the divergence — v4 reports its own
            // refused rows and v5 reports nothing. Asserted, not diffed.
            if !dedupe {
                compare_warnings(name, gv, wv, failures);
            }
            continue;
        }
        if dedupe && REPLAY_DEDUPE_SUMMARY_KEYS.contains(&k.as_str()) {
            continue;
        }
        if gv != wv {
            failures.push(format!("[{name}] summary.{k}: rust {gv} vs oracle {wv}"));
        }
    }

    // ── 2. The named residuals, asserted in BOTH directions. ─────────────────
    assert_stats_gap(name, got, want, failures);
    if dedupe {
        assert_replay_dedupe(
            name,
            zip,
            temp_root,
            got,
            want,
            summary,
            want_summary,
            failures,
        );
    }

    // ── 3. Every table, row by row, after normalization. ─────────────────────
    let residual: HashSet<&str> = PHASE_ORDER_RESIDUAL
        .iter()
        .filter(|(c, _, _)| *c == name)
        .map(|(_, _, t)| *t)
        .collect();
    let mut stats_masked: HashSet<&str> = V5_STATS_GAP
        .iter()
        .filter(|(c, _)| *c == name)
        .map(|(_, m)| *m)
        .collect();
    // A dedupe case's uploads mount carries different rollups for a DIFFERENT
    // reason than `V5_STATS_GAP`'s: v5 never calls the bridge for a carried file,
    // so there is no `refreshStats` to skip. `assert_replay_dedupe` pins the
    // direction (v4 counts strictly more).
    if dedupe {
        stats_masked.insert("Quilltap Uploads");
    }
    // Blank the three rollup columns on exactly the named mount-point rows —
    // nothing else in the table, and nothing in any other case.
    let mask_stats = |rows: &mut Vec<Value>| {
        for row in rows.iter_mut() {
            let named = row
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| stats_masked.contains(n));
            if !named {
                continue;
            }
            if let Some(m) = row.as_object_mut() {
                for col in STATS_COLUMNS {
                    if m.contains_key(*col) {
                        m.insert((*col).to_string(), Value::String("<v5-stats-gap>".into()));
                    }
                }
            }
        }
    };

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
    let shas_got = derived_shas(got, &literals);
    let shas_want = want
        .as_object()
        .map(|_| {
            let as_map: BTreeMap<String, BTreeMap<String, Vec<Value>>> =
                serde_json::from_value(want.clone()).unwrap_or_default();
            derived_shas(&as_map, &literals)
        })
        .unwrap_or_default();

    for (partition, tables) in got {
        for (table, rows) in tables {
            // The ruled dedupe divergence: these five hold different rows by
            // design. `assert_replay_dedupe` above states exactly how, in both
            // directions, instead of diffing them.
            if dedupe && REPLAY_DEDUPE_TABLES.contains(&(partition.as_str(), table.as_str())) {
                continue;
            }
            let mut n_got = Normalizer::new(literals.clone(), shas_got.clone());
            let mut n_want = Normalizer::new(literals.clone(), shas_want.clone());
            let want_rows = want[partition][table]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut g_rows = rows.clone();
            let mut w_rows = want_rows;
            if table == "doc_mount_points" && !stats_masked.is_empty() {
                mask_stats(&mut g_rows);
                mask_stats(&mut w_rows);
            }
            let g = n_got.value(&Value::Array(g_rows));
            let wnt = n_want.value(&Value::Array(w_rows));
            // The documented phase-order residual: same rows, different
            // insertion order. Asserted in both directions instead of diffed.
            if partition == "mountIndex" && residual.contains(table.as_str()) {
                failures.extend(assert_phase_order_residual(
                    name,
                    table,
                    rows,
                    &want[partition][table]
                        .as_array()
                        .cloned()
                        .unwrap_or_default(),
                    &literals,
                    &shas_got,
                    &shas_want,
                ));
                continue;
            }
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
