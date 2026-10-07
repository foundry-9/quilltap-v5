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
    // [P4.D152] The bug-117 archive replays a legacy-disk-key `portrait.png` —
    // the exact shape this residual describes — so the two file phases write its
    // content row, link and blob at different points in the insertion order. Not
    // a new divergence; the ruled one, reached by a third archive.
    // `doc_mount_folders` is NOT here: its orders measurably agree on this
    // archive (the carve-out's own staleness check says so). `main.files` is not
    // here either — it is this case's whole question (bug 117's `sha256`) and is
    // diffed in order like everything else.
    (
        "restore_bug117_new_account",
        "mountIndex",
        "doc_mount_blobs",
    ),
    (
        "restore_bug117_new_account",
        "mountIndex",
        "doc_mount_file_links",
    ),
    (
        "restore_bug117_new_account",
        "mountIndex",
        "doc_mount_files",
    ),
    ("restore_new_account", "mountIndex", "doc_mount_blobs"),
    ("restore_new_account", "mountIndex", "doc_mount_file_links"),
    ("restore_new_account", "mountIndex", "doc_mount_files"),
    // [P4.D158] Same archive as `restore_new_account` bar four values inside
    // existing columns, so its `new-account` restore replays the same legacy
    // disk-key file and the two file phases place its content row, link and blob
    // at the same differing points. The ruled divergence, reached by a fourth
    // archive shape.
    (
        "restore_bag_keys_new_account",
        "mountIndex",
        "doc_mount_blobs",
    ),
    (
        "restore_bag_keys_new_account",
        "mountIndex",
        "doc_mount_file_links",
    ),
    (
        "restore_bag_keys_new_account",
        "mountIndex",
        "doc_mount_files",
    ),
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
    // [P4.D51] `restore_uploads_new_account` CONVERGED on bug 12's re-ingest
    // (its `new_account` mode remaps mounts, so no `restored/` collision), but it
    // replays a legacy-disk-key file and so retains the SAME ruled phase-order
    // insertion difference as `restore_new_account`.
    (
        "restore_uploads_new_account",
        "mountIndex",
        "doc_mount_blobs",
    ),
    (
        "restore_uploads_new_account",
        "mountIndex",
        "doc_mount_file_links",
    ),
    (
        "restore_uploads_new_account",
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
    // [P4.D51] Converged off REPLAY_DEDUPE; its uploads mount hits the same stale
    // rollup (v4 refreshes `fileCount`/`totalSizeBytes`, v5 does not).
    ("restore_uploads_new_account", "Quilltap Uploads"),
    // [P4.D152] The bug-117 archive replays a legacy-disk-key file into the
    // target's own uploads mount, so it reaches the same stale rollup. Not a new
    // gap — the existing deferral, reached by a third archive shape. (The
    // `replace` mode of this archive is deliberately not a case at all: there the
    // archived mount row restores verbatim and v5 keeps the ARCHIVE's rollups,
    // which this zero-fileCount assertion cannot express. See the oracle's list.)
    ("restore_bug117_new_account", "Quilltap Uploads"),
    // [P4.D158] The bag-keys archive IS `restore-archive.zip` plus four JSON
    // edits, so its `new-account` restore replays the same one
    // `files/portrait.png` into the target's own uploads mount and hits the same
    // stale rollup. A fifth case of the shape, not a fifth gap — the edits are
    // four values inside existing columns and touch no file at all.
    ("restore_bag_keys_new_account", "Quilltap Uploads"),
];

/// The columns [`V5_STATS_GAP`] makes incomparable on its named rows.
const STATS_COLUMNS: &[&str] = &["fileCount", "chunkCount", "totalSizeBytes"];

/// ## ⚠ THE RULED PHASE-ORDER DIVERGENCE (P4.d23 → bug 12, v4 PARTIALLY converged)
///
/// v4 USED to re-ingest every user file in the archive unconditionally, refusing
/// its own archived link rows on the second generation. **v4 has since CONVERGED**
/// on that half (`3bb664f0`, bug 12: it adopted v5's `carried_store_rows` skip
/// check — `orchestrator.rs`). The storageKeys now agree and the per-carried-file
/// re-ingest is gone, so the gen-2 archive restores identically on both sides and
/// is a PLAIN equality (it is no longer in this list).
///
/// **What v4 kept, and this list now pins.** v4 did NOT move its file phase from
/// `22a-bis` to v5's after-the-doc-store slot — the human ruling of 2026-07-26
/// kept v5's later slot, and v4 names the phase-order repair out of scope itself
/// (`found-bugs.md:400-402`). So on the two archives whose file phase still races
/// into `restored/`, v4 diverges and v5 is clean:
///   - **uploads**: v4's replay wins `restored/`, so the doc-store folder phase
///     collides — v4 warns `Failed to restore doc-store folder "restored": UNIQUE
///     constraint failed` and restores one FEWER folder. v5 restores the tree
///     whole.
///   - **compact**: additionally, v4 cannot dedup the archive's >3 MB (multi-chunk)
///     carried file (the sparse-array export boundary makes its skip check miss),
///     so it invents a PHANTOM doc-store copy — one extra blob/file/link the
///     archive never linked there. v5 restores exactly the one atlas file the
///     archive carries.
///
/// Both are v5-ahead under the standing 2026-08-03 backup/restore ruling ("v5
/// FIXES v4's bugs in this family"). Asserted in BOTH directions by
/// [`assert_replay_dedupe`]: v5 must be clean AND v4 must still diverge; if v4
/// fully converges (adopts the later slot and the >3 MB dedup) the retire
/// tripwire fires. **The compact >3 MB phantom is a P4.D51 discovery; the
/// uploads/compact phase-order collision + the >3 MB phantom are both queued on
/// the post-5.0 v4-side list.**
const REPLAY_DEDUPE: &[&str] = &[
    // `replace` mode preserves the archive's mount ids, so v4's replay races into
    // the SAME `restored/` folder the doc-store phase then re-creates → collision.
    // (`new_account` remaps every mount, so there is no collision and the uploads
    // archive converges fully — it is NOT here.)
    "restore_uploads_replace",
    // [P4.D46 → P4.D51] The compact archive carries a >3 MB store-backed file; v4
    // fails to dedup it and phantoms a doc-store copy, in BOTH modes.
    "restore_compact_replace",
    "restore_compact_new_account",
    // [P4.147] the same archive into a FRESH target: the >3 MB phantom is the
    // project-bound file, which never touches the Uploads pointer, so #142's
    // fix leaves it exactly as `restore_compact_replace` has it.
    "restore_compact_fresh_replace",
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

/// ## [P4.158 R-F] A [`REPLAY_DEDUPE`] case NARROWED to the tables that differ
///
/// `restore_compact_fresh_replace` (P4.147) joined the list wholesale, "most of
/// its diff off". Measured at P4.158 with the carve lifted: its warnings and
/// every summary counter already compare EQUAL — the fresh target's portrait
/// never reaches v4's file phase (`FRESH_TARGET_UPLOADS`), so v4 never races
/// into `restored/` and has no folder collision to report — and of the five
/// tables only FOUR carry the divergence, each for the same reason: the
/// over-3 MB phantom copy v4 re-ingests of the carried `atlas-plates.bin` into the PROJECT
/// store (one extra `doc_mount_blobs`, `doc_mount_files` and
/// `doc_mount_file_links` row, and `main.files`' `storageKey` / `sha256`
/// naming the phantom instead of the archive's blob). `doc_mount_folders`
/// differed only because `FRESH_TARGET_UPLOADS`' carve left behind the
/// `restored` folder v5's portrait replay created — a carve gap, fixed there —
/// so it comes back under diff, and the case's warnings and summary keys with
/// it. `assert_replay_dedupe` still holds the case both ways.
const REPLAY_DEDUPE_NARROWED: &[(&str, &[(&str, &str)])] = &[(
    "restore_compact_fresh_replace",
    &[
        ("main", "files"),
        ("mountIndex", "doc_mount_blobs"),
        ("mountIndex", "doc_mount_files"),
        ("mountIndex", "doc_mount_file_links"),
    ],
)];

/// The tables [`REPLAY_DEDUPE`] carves on `name`.
fn replay_dedupe_tables(name: &str) -> &'static [(&'static str, &'static str)] {
    REPLAY_DEDUPE_NARROWED
        .iter()
        .find(|(c, _)| *c == name)
        .map(|(_, t)| *t)
        .unwrap_or(REPLAY_DEDUPE_TABLES)
}

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
        // [P4.D46] The REAL wall clock, not 0: step 25's reconcile derives its
        // stale-chat cutoff from this, and v4's oracle uses Date.now() — with
        // an epoch clock nothing is ever stale and the stale-chunk clearing
        // arm silently diverges (seen on the first compact regen).
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
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
        // P4.D208 (v4 bug 158, `da9c4f34f`): `restore-archive.zip`'s instance
        // with its two chats given the two columns — the first SEEDED, the
        // second a real summary that quotes the scenario. A derivation, because
        // every other committed archive has both columns NULL on both chats and
        // cannot see the strip at all.
        "restore_bug158_replace" => "restore-archive-bug158.zip",
        // P4.D226 (v4 `4d370a90f`, #75): the four rows v4's legacy Concierge
        // derive table distinguishes, plus a 4.10 row it leaves alone. Built
        // by `harness/oracle/fixtures/derive-restore-archive-concierge-legacy.py`.
        "restore_concierge_legacy_replace" => "restore-archive-concierge-legacy.zip",
        // P4.130 (P4.124 item 14's plant): `restore-archive.zip` plus one
        // message-less clone carrying `conciergeMode: 'bogus'` — skipped by
        // BOTH sides with the ZodError bytes in `summary.warnings`. Built by
        // `harness/oracle/fixtures/derive-restore-archive-concierge-bogus.py`.
        "restore_concierge_bogus_replace" => "restore-archive-concierge-bogus.zip",
        // P4.143 item 2 (the restore's serde arm): `restore-archive.zip` plus
        // one message-less clone carrying `scenarioText: 5` with valid
        // Concierge columns — skipped by BOTH sides, v4 with the ZodError, v5
        // with serde's sentence (`classify_restore_serde_arm`). Built by
        // `harness/oracle/fixtures/derive-restore-archive-chat-serde-arm.py`.
        "restore_chat_serde_arm_replace" => "restore-archive-chat-serde-arm.zip",
        // P4.D251 (v4 `07b8f0209`): `restore-archive.zip` plus five settings-row
        // clones carrying the retired `impersonationVoiceRewrite` boolean in
        // its shapes (true / false / 1 / beside an explicit 'always') and one
        // current record — the restore's legacy voice translation, which no
        // other committed archive can see. Built by
        // `harness/oracle/fixtures/derive-restore-archive-voice-legacy.py`.
        "restore_voice_legacy_replace" => "restore-archive-voice-legacy.zip",
        "restore_legacy_archive" => "restore-archive-legacy.zip",
        "restore_minimal" => "restore-archive-minimal.zip",
        "restore_new_account" => "restore-archive.zip",
        "restore_uploads_replace" | "restore_uploads_new_account" => "restore-archive-uploads.zip",
        "restore_gen2_replace" | "restore_gen2_new_account" => "restore-archive-gen2.zip",
        "restore_memory_graph_replace" | "restore_memory_graph_new_account" => {
            "restore-archive-memory-graph.zip"
        }
        "restore_orphan_links_replace" => "restore-archive-orphan-links.zip",
        // [P4.D152] bug 117 — the archive that carries a `files.sha256` lie on
        // BOTH file branches (the legacy disk-key `portrait.png` through the
        // replay, the store-backed `plate.png` through carried-store-rows).
        // Every other committed archive carries a `sha256` that already agrees
        // with its bytes, so none of them can tell "copy the archive's value"
        // from "ask the bridge" apart.
        "restore_bug117_new_account" => "restore-archive-bug117.zip",
        // [P4.D46] The compact restore tail (24a + 25).
        "restore_compact_replace" | "restore_compact_new_account" => "restore-archive-compact.zip",
        // [P4.D126] bug 103 — the connection-profile columns an older archive
        // predates. See the oracle case list for what the six profiles span and
        // why the `multiCharacterPrefill` half is pinned in
        // `restore_vintage_state` instead.
        "restore_legacy_profiles_replace" => "restore-archive-legacy-profiles.zip",
        // [P4.D145] bug 114 — the quiet duplicate-folder drop. All eleven other
        // committed archives carry exactly ONE folder row (measured
        // 2026-09-02), so none of them can see it.
        "restore_duplicate_folders_replace" => "restore-archive-duplicate-folders.zip",
        // [P4.D158] v4 `2edd823c0` — the four 4.9/4.10 additions that ride
        // INSIDE an existing column (a widened enum domain and three JSON bags).
        // None of the thirteen other committed archives carries ANY of the four
        // (measured 2026-09-05), which is exactly the blind spot v4 names: a new
        // column announces itself with a migration; a key inside a bag does not.
        "restore_bag_keys_replace" | "restore_bag_keys_new_account" => {
            "restore-archive-bag-keys.zip"
        }
        // [P4.147, dogfood #142] the three archives that carry their own
        // Quilltap Uploads store, restored into a fresh target with NO pointer
        // alignment — see `FRESH_TARGET_UPLOADS`.
        "restore_uploads_fresh_replace" => "restore-archive-uploads.zip",
        "restore_compact_fresh_replace" => "restore-archive-compact.zip",
        "restore_gen2_fresh_replace" => "restore-archive-gen2.zip",
        // [P4.147 item 8] the fallback arm's whole property bag — built by
        // `harness/oracle/fixtures/derive-restore-archive-bag-nulls.py`.
        "restore_bag_nulls_replace" => "restore-archive-bag-nulls.zip",
        // [P4.147 items 9 + 10(b)] seven archived informs + one malformed
        // message — `derive-restore-archive-informs.py`.
        "restore_informs_replace" => "restore-archive-informs.zip",
        // [P4.147 item 10(a)+(c)] the full archive into a column-renamed
        // target — see `renamed_columns`.
        "restore_sqlite_tail_replace" => "restore-archive.zip",
        // [P4.158 item 1, R-A] three preserved stores each missing
        // `description.md` — `derive-restore-archive-damaged-store.py`.
        "restore_damaged_store_replace" => "restore-archive-damaged-store.zip",
        // [P4.158 item 2, R-B] a second claimant on Lorian's vault and on the
        // project's store — `derive-restore-archive-two-claimants.py`.
        "restore_two_claimants_replace" => "restore-archive-two-claimants.zip",
        // [P4.158 item 2, R-B] Lorian's vault id duplicated as a `documents`
        // row — `derive-restore-archive-dup-store-id.py`.
        "restore_dup_store_id_replace" => "restore-archive-dup-store-id.zip",
        // [P4.158 item 3, R-C] gen2 plus one shared legacy preset, into a fresh
        // target — `derive-restore-archive-general-pointer.py`.
        "restore_general_pointer_fresh_replace" => "restore-archive-general-pointer.zip",
        // [P4.158 R-G + R-H] every phase's per-row catch, planted — see
        // `renamed_columns_in`.
        "restore_phase_warns_replace" => "restore-archive-legacy.zip",
        // [P4.161] whole-row refusals — `derive-restore-archive-{memory,
        // inform,entity}-refusals.py` (dogfood #152, §S.2, P4.155's R-B).
        "restore_memory_refusals_replace" => "restore-archive-memory-refusals.zip",
        "restore_inform_refusals_replace" => "restore-archive-inform-refusals.zip",
        "restore_entity_refusals_replace" => "restore-archive-entity-refusals.zip",
        // [P4.161 Tier 2] one refusing row per landed Tier 2 kind —
        // `derive-restore-archive-kind-refusals.py`.
        "restore_kind_refusals_replace" => "restore-archive-kind-refusals.zip",
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
/// [P4.D145] Does this case need the bug-114 unique index on the TARGET before
/// restoring? Mirrors the oracle's `collapseFolders` flag exactly.
fn collapses_folders(name: &str) -> bool {
    name == "restore_duplicate_folders_replace"
}

fn aligns_uploads_pointer(name: &str) -> bool {
    matches!(
        name,
        "restore_uploads_replace" | "restore_gen2_replace" | "restore_compact_replace"
    )
}

/// [P4.147 item 10(c)] The main-partition columns this case RENAMES on the
/// target before the baseline (`(table, from, to)`) — mirrors the oracle's
/// `renameColumns` exactly. The P4.131 plant shape: every restore insert into
/// the table then fails on SQLite's own `table … has no column named …`, so
/// the restore's per-row catches are reached with a REAL database error.
fn renamed_columns(name: &str) -> &'static [(&'static str, &'static str, &'static str)] {
    match name {
        "restore_sqlite_tail_replace" => &[
            (
                "chats",
                "rightPaneVerticalSplit",
                "rightPaneVerticalSplitPlanted",
            ),
            ("chat_documents", "displayTitle", "displayTitlePlanted"),
        ],
        _ => &[],
    }
}

/// [P4.158 R-G] The per-partition column-rename plant (`(partition file, table,
/// from, to)`) — mirrors the oracle's `renameColumnsIn` exactly: ONE written
/// column renamed in every phase's table, so each restore insert there fails on
/// SQLite's own error and the per-row catch (warning + WARN) is reached.
const PHASE_WARNS_PLANT: &[(&str, &str, &str, &str)] = &[
    ("quilltap.db", "tags", "nameLower", "nameLowerPlanted"),
    (
        "quilltap.db",
        "connection_profiles",
        "sortIndex",
        "sortIndexPlanted",
    ),
    ("quilltap.db", "image_profiles", "tags", "tagsPlanted"),
    ("quilltap.db", "embedding_profiles", "tags", "tagsPlanted"),
    (
        "quilltap.db",
        "memories",
        "reinforcedImportance",
        "reinforcedImportancePlanted",
    ),
    (
        "quilltap.db",
        "prompt_templates",
        "content",
        "contentPlanted",
    ),
    (
        "quilltap.db",
        "roleplay_templates",
        "narrationDelimiters",
        "narrationDelimitersPlanted",
    ),
    (
        "quilltap.db",
        "provider_models",
        "experimental",
        "experimentalPlanted",
    ),
    (
        "quilltap.db",
        "projects",
        "officialMountPointId",
        "officialMountPointIdPlanted",
    ),
    (
        "quilltap.db",
        "groups",
        "officialMountPointId",
        "officialMountPointIdPlanted",
    ),
    ("quilltap.db", "plugin_configs", "enabled", "enabledPlanted"),
    ("quilltap.db", "folders", "path", "pathPlanted"),
    (
        "quilltap.db",
        "character_plugin_data",
        "data",
        "dataPlanted",
    ),
    (
        "quilltap.db",
        "conversation_annotations",
        "characterName",
        "characterNamePlanted",
    ),
    (
        "quilltap.db",
        "vector_entries",
        "embedding",
        "embeddingPlanted",
    ),
    (
        "quilltap.db",
        "conversation_chunks",
        "participantNames",
        "participantNamesPlanted",
    ),
    (
        "quilltap.db",
        "tfidf_vocabularies",
        "includeBigrams",
        "includeBigramsPlanted",
    ),
    (
        "quilltap.db",
        "embedding_status",
        "embeddedAt",
        "embeddedAtPlanted",
    ),
    (
        "quilltap.db",
        "text_replacement_rules",
        "sortOrder",
        "sortOrderPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "doc_mount_points",
        "conversionError",
        "conversionErrorPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "doc_mount_folders",
        "parentId",
        "parentIdPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "doc_mount_files",
        "fileType",
        "fileTypePlanted",
    ),
    (
        "quilltap-mount-index.db",
        "doc_mount_file_links",
        "description",
        "descriptionPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "doc_mount_documents",
        "plainTextLength",
        "plainTextLengthPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "doc_mount_chunks",
        "headingContext",
        "headingContextPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "project_doc_mount_links",
        "projectId",
        "projectIdPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "group_doc_mount_links",
        "groupId",
        "groupIdPlanted",
    ),
    (
        "quilltap-mount-index.db",
        "group_character_members",
        "characterId",
        "characterIdPlanted",
    ),
    (
        "quilltap-llm-logs.db",
        "llm_logs",
        "provider",
        "providerPlanted",
    ),
];

fn renamed_columns_in(
    name: &str,
) -> &'static [(&'static str, &'static str, &'static str, &'static str)] {
    match name {
        "restore_phase_warns_replace" => PHASE_WARNS_PLANT,
        _ => &[],
    }
}

/// [P4.158 R-G] Raw SQL planted on a partition — mirrors the oracle's
/// `plantSqlIn`. v5's chat-settings insert drops a column the table lacks
/// (`tolerant_insert`), so its catch is reached by a trigger, not a rename.
fn plant_sql_in(name: &str) -> &'static [(&'static str, &'static str)] {
    match name {
        "restore_phase_warns_replace" => &[
            (
                "quilltap.db",
                "CREATE TRIGGER \"planted_chat_settings_failure\" BEFORE INSERT ON \"chat_settings\" \
                 BEGIN SELECT RAISE(ABORT, 'planted chat settings failure'); END",
            ),
            // Characters by trigger as well: v5's vault resolvers read NAMED
            // columns, so a rename would break 22f-bis's reads (v4's are
            // `SELECT *`) and miss R-H's no-mount arm.
            (
                "quilltap.db",
                "CREATE TRIGGER \"planted_characters_failure\" BEFORE INSERT ON \"characters\" \
                 BEGIN SELECT RAISE(ABORT, 'planted characters failure'); END",
            ),
            // No General pointer on the target: the SHARED legacy preset takes
            // 22f-bis's no-mount arm too (R-H) instead of a vault write into
            // the target's wiped General store.
            (
                "quilltap.db",
                "DELETE FROM \"instance_settings\" WHERE \"key\" = 'generalMountPointId'",
            ),
            // `save_meta` pre-reads every named meta column (v4's lookup does
            // not), so a rename would trip the read first.
            (
                "quilltap.db",
                "CREATE TRIGGER \"planted_vector_meta_failure\" BEFORE INSERT ON \"vector_indices\" \
                 BEGIN SELECT RAISE(ABORT, 'planted vector index meta failure'); END",
            ),
        ],
        _ => &[],
    }
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
    // [P4.158 R-G] the phase plant refuses every memory on BOTH sides (its
    // characters never restore); the census and the row diff cover it.
    // [P4.161] the memory-refusals archive refuses eight of its memories on
    // BOTH sides by design (none of them is an edge target);
    // `assert_refusals_restored` names exactly which land.
    if name == "restore_phase_warns_replace" || name == "restore_memory_refusals_replace" {
        return;
    }
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
    let _serial = serial();
    let Some(cases) = read_cases() else {
        eprintln!("SKIP: QT_ORACLE_SYSTEM_RESTORE unset");
        return;
    };

    let mut failures: Vec<String> = Vec::new();
    let mut seen = 0usize;
    let mut repo_log_cases = 0usize;
    let mut restore_log_cases = 0usize;
    let mut fresh_store_carved = 0usize;

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
        // [P4.D145] Give the target the bug-114 unique index before the
        // baseline is dumped, exactly as v4's oracle runs its own migration at
        // the same point; both apps really do boot (v4's migration runner /
        // v5's boot ensure) before anyone restores. `generateDDL` cannot
        // express a COALESCE index, but since P4.153 (dogfood #149) the
        // provisioner replays v4's MIGRATION-created index family
        // (`migration_indexes.json`), which carries it — so a fresh target is
        // already indexed (`AlreadyIndexed`), as a real v4 first boot leaves
        // it; a target that predates P4.153 still runs the collapse (`Ran`).
        if collapses_folders(name) {
            let w = quilltap_core::db::Writer::open_writable(
                &instance.join("quilltap.db"),
                TEST_PEPPER,
            )
            .expect("open target to materialize the folders index");
            let outcome =
                quilltap_core::db::folders_unique_path_repair::ensure_folders_unique_path_index(
                    w.connection(),
                    "2026-09-02T00:00:00.000Z",
                )
                .expect("collapse ensure on target");
            assert!(
                matches!(
                    outcome,
                    quilltap_core::db::folders_unique_path_repair::CollapseOutcome::Ran { .. }
                        | quilltap_core::db::folders_unique_path_repair::CollapseOutcome::AlreadyIndexed
                ),
                "[{name}] a fresh target must come out indexed — got {outcome:?}"
            );
        }
        if aligns_uploads_pointer(name) {
            align_uploads_pointer(&instance, &zip, &host.temp_dir());
        }
        // [P4.147 item 10(c)] the column-rename plant, before the baseline —
        // exactly where the oracle runs its own `ALTER TABLE … RENAME COLUMN`.
        if !renamed_columns(name).is_empty() {
            let w = quilltap_core::db::Writer::open_writable(
                &instance.join("quilltap.db"),
                TEST_PEPPER,
            )
            .expect("open target to plant the column renames");
            for (table, from, to) in renamed_columns(name) {
                w.connection()
                    .execute_batch(&format!(
                        "ALTER TABLE \"{table}\" RENAME COLUMN \"{from}\" TO \"{to}\""
                    ))
                    .expect("plant the column rename");
            }
        }
        // [P4.158 R-G] the per-partition plant, at the same point.
        for (file, table, from, to) in renamed_columns_in(name) {
            let w = quilltap_core::db::Writer::open_writable(&instance.join(file), TEST_PEPPER)
                .expect("open a target partition to plant a column rename");
            w.connection()
                .execute_batch(&format!(
                    "ALTER TABLE \"{table}\" RENAME COLUMN \"{from}\" TO \"{to}\""
                ))
                .expect("plant the column rename");
        }
        for (file, sql) in plant_sql_in(name) {
            let w = quilltap_core::db::Writer::open_writable(&instance.join(file), TEST_PEPPER)
                .expect("open a target partition to plant a trigger");
            w.connection()
                .execute_batch(sql)
                .expect("plant the trigger");
        }
        let got_pre = read_state(&instance);
        compare_baseline(name, &got_pre, &case["preState"], &mut failures);
        let db = reopen_instance(&instance);

        // [P4.143 Tier 2 item 10] Every case starts from an EMPTY process-global
        // buffer (the restore logs on the WRITER thread). Since P4.158 every
        // case reads it — the v5-only claim WARN is pinned on all of them.
        let log_buf = {
            let buf = global_capture();
            buf.lock().unwrap().clear();
            buf
        };
        let summary = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(restore(
                &db,
                &host,
                &zip,
                mode_for(name),
                SINGLE_USER_ID,
                Default::default(),
            ))
            .expect("restore succeeded");

        // Dump AFTER dropping the Db so every writer transaction has committed.
        drop(db);
        let mut got_state = read_state(&instance);
        let mut want_owned = case["state"].clone();
        // [the `94fbb1ae3` boot-hardness unification] the ruled
        // index-keyed-embedding divergence, both ways, then carved from the
        // dumps, the warnings and both log censuses (its own case copy).
        let mut case_owned = case.clone();
        let mut index_keyed_drops = Vec::new();
        carve_index_keyed_embedding(
            name,
            &mut got_state,
            &mut want_owned,
            &mut case_owned,
            &mut index_keyed_drops,
            &mut failures,
        );
        let case = &case_owned;

        // [P4.147] The two ruled divergences, asserted both ways on the RAW
        // dumps and then carved — BEFORE any normalization, so the
        // `<minted-N>` first-encounter labels of the remaining rows line up.
        let mut summary_carve = SummaryCarve {
            drop_v4_warnings: index_keyed_drops,
            ..SummaryCarve::default()
        };
        if mode_for(name) == RestoreMode::Replace {
            if let Some(shared) = carve_fresh_store_residual(
                name,
                &zip,
                &host.temp_dir(),
                &mut got_state,
                &mut want_owned,
                &mut failures,
            ) {
                fresh_store_carved += 1;
                summary_carve.shared_content = shared > 0;
            }
        }
        // [P4.158 R-A] the backfilled managed files, both ways.
        summary_carve.preserve_backfill = assert_preserve_backfill(
            name,
            &zip,
            &host.temp_dir(),
            &got_state,
            &want_owned,
            &mut failures,
        );
        // [P4.161 Tier 2 item 10] the backfill's shared content rows, both
        // ways, then collapsed on v4's side.
        collapse_backfill_shared_content(
            name,
            &zip,
            &host.temp_dir(),
            &got_state,
            &mut want_owned,
            &mut failures,
        );
        carve_fresh_target_uploads(
            name,
            &zip,
            &host.temp_dir(),
            &summary,
            &case["summary"],
            &mut got_state,
            &want_owned,
            &mut summary_carve,
            &mut failures,
        );
        // [P4.158 R-C] the shared legacy archetype's General store.
        carve_general_pointer_preapply(name, &mut got_state, &mut want_owned, &mut failures);
        let want_state = &want_owned;

        // v5 alone: the restored memory graph must be internally closed. Run for
        // EVERY case, not just the memory-graph ones — the other archives' edge
        // sets are empty, so it is cheap there and guards the id preservation.
        assert_memory_graph_intact(name, &zip, &host.temp_dir(), &got_state, &mut failures);

        // [P4.D152] bug 117's own comparand, taken from the RAW dumps.
        assert_bug117_stored_sha(name, &got_state, want_state, &mut failures);

        // [P4.D158] v4 `2edd823c0`'s four bag-key arms, within-tree.
        assert_bag_keys_survive(name, &got_state, &mut failures);

        // [P4.130] the refused-chat plant, by name.
        assert_bogus_concierge_chat_skipped(
            name,
            &summary,
            &case["summary"],
            &got_state,
            want_state,
            &mut failures,
        );

        // [P4.143 Tier 2 item 10] the refused chat create's repository lines.
        let lines = std::mem::take(&mut *log_buf.lock().unwrap());
        if case.get("repoLogs").is_some() {
            let serde = (name == SERDE_ROOM_CASE).then_some(("scenarioText", SERDE_ROOM_V5_PREFIX));
            // Restore runs OUTSIDE `withStrictRepositoryFailures`: no line
            // carries `strictFailures` on v4 either.
            compare_repo_logs(name, &case["repoLogs"], &lines, serde, &[], &mut failures);
            repo_log_cases += 1;
        }
        // [P4.147 → P4.158 R-G] the restore's own lines (the whole census) on
        // EVERY case, byte for byte.
        compare_restore_logs(
            name,
            &case["logs"],
            &lines,
            &summary,
            &case["summary"],
            &summary_carve,
            &archive_literals(&zip, &host.temp_dir()),
            &mut failures,
        );
        restore_log_cases += 1;
        // [P4.158 R-A/R-B] the v5-only claim + backfill WARNs, on every case.
        assert_claimed_store_warns(name, &lines, &mut failures);

        // [P4.147 item 8] the fallback arm's whole property bag, by name.
        assert_bag_nulls_survive(
            name,
            &zip,
            &host.temp_dir(),
            &got_state,
            want_state,
            &mut failures,
        );

        // [P4.147 item 9] the archived informs, by name.
        assert_informs_restored(name, &got_state, want_state, &mut failures);
        assert_refusals_restored(name, &got_state, want_state, &mut failures);

        // [P4.143 item 2] the serde-arm plant, by name.
        assert_serde_room_skipped(
            name,
            &summary,
            &case["summary"],
            &got_state,
            want_state,
            &mut failures,
        );

        // [P4.106 item 6] the pre-4.10-archive arm.
        assert_pre_410_archive_restores_no_informs(
            name,
            &summary,
            &case["summary"],
            &got_state,
            want_state,
            &mut failures,
        );

        let literals = archive_literals(&zip, &host.temp_dir());
        compare_case(
            name,
            &summary,
            case,
            &summary_carve,
            &got_state,
            want_state,
            literals,
            &zip,
            &host.temp_dir(),
            &mut failures,
        );
    }

    // 18 + 1 = 19: P4.D208's bug-158 arm (the seeded summary a stale backup
    // carries, stripped on the way in). 19 + 1 = 20: P4.D226's legacy
    // Concierge arm (the three states derived from the legacy pair). 20 + 1 =
    // 21: P4.130's refused-chat arm (a `conciergeMode` outside the enum). 21 +
    // 1 = 22: P4.143's serde-arm plant (a numeric `scenarioText`). 22 + 1 =
    // 23: P4.D251's voice-legacy arm (the retired `impersonationVoiceRewrite`
    // boolean in its shapes, translated on restore). 23 + 6 = 29: P4.147's
    // three fresh-target arms (#142), the bag-nulls arm, the informs arm and
    // the SQLite-tail plant.
    // 34 + 3 = 37: P4.161's three whole-row refusal arms. 37 + 1 = 38: its
    // Tier 2 kind-refusals arm.
    assert_eq!(
        seen, 38,
        "expected all thirty-eight restore cases in the oracle (ten + the #58 orphan-links arm \
         + P4.D46's two compact arms + P4.D126's bug-103 legacy-profiles arm \
         + P4.D145's bug-114 duplicate-folders arm + P4.D152's bug-117 arm \
         + P4.D158's two bag-key arms + P4.D208's bug-158 arm \
         + P4.D226's legacy-Concierge arm + P4.130's refused-chat arm \
         + P4.143's serde-arm plant + P4.D251's voice-legacy arm \
         + P4.147's three fresh-target arms, bag-nulls, informs and SQLite-tail arms \
         + P4.158's damaged-store, two-claimants, dup-store-id, general-pointer and phase-warns arms \
         + P4.161's memory-, inform-, entity- and kind-refusal arms)"
    );
    // [P4.147] Every ruled #141 case actually carved (red-first measured, then
    // both directions held) — the arm cannot go vacuous.
    assert_eq!(
        fresh_store_carved, FRESH_STORE_CARVED_CASES,
        "replace cases whose v4 dump carried a #141 fresh store to carve"
    );
    assert!(
        failures.is_empty(),
        "{} restore-state difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    // [P4.143 Tier 2 item 10] Both chat refusal arms compared their repository
    // lines (the Concierge-bogus arm byte for byte, the serde arm carved).
    assert_eq!(
        repo_log_cases, 2,
        "cases that compared the refused chat create's repository lines"
    );
    // [P4.158 R-G] every case compared the census.
    assert_eq!(
        restore_log_cases, seen,
        "cases that compared the restore's log census (every case, P4.158 R-G)"
    );
}

/// [P4.130] **The refused-chat arm** (P4.124 item 14's family-level plant).
/// `restore-archive-concierge-bogus.zip` carries one chat, `c…0005`, whose
/// `conciergeMode` is `'bogus'`: v4's `repos.chats.create` validation throws,
/// and the per-chat catch skips it with `Failed to restore chat "The Bogus
/// Room": <ZodError.message>`. The whole-table diff and `compare_warnings`
/// already compare both halves; this pins them by NAME so the arm cannot go
/// vacuous — the chat is absent on BOTH sides, and BOTH warning lists carry
/// exactly one such warning whose tail is the rendered `invalid_value` issue
/// (v4's bytes, `concierge_columns_zod_error` on v5's side).
fn assert_bogus_concierge_chat_skipped(
    name: &str,
    got_summary: &RestoreSummary,
    want_summary: &Value,
    got_state: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want_state: &Value,
    failures: &mut Vec<String>,
) {
    if name != "restore_concierge_bogus_replace" {
        return;
    }
    const BOGUS: &str = "c1000000-0000-4000-8000-000000000005";
    const HEAD: &str =
        "Failed to restore chat \"The Bogus Room\": [\n  {\n    \"code\": \"invalid_value\"";
    let got_chats = got_state
        .get("main")
        .and_then(|m| m.get("chats"))
        .map(|rows| rows.iter().any(|r| r["id"] == BOGUS));
    let want_chats = want_state["main"]["chats"]
        .as_array()
        .map(|rows| rows.iter().any(|r| r["id"] == BOGUS));
    if got_chats != Some(false) || want_chats != Some(false) {
        failures.push(format!(
            "[{name}] the bogus chat must be ABSENT on both sides (v5 present: {got_chats:?}, \
             v4 present: {want_chats:?})"
        ));
    }
    let got_warn = json!(got_summary.warnings);
    for (side, warnings) in [("v5", &got_warn), ("v4", &want_summary["warnings"])] {
        let hits = warnings
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .filter(|w| {
                        w.starts_with(HEAD) && w.contains("\"path\": [\n      \"conciergeMode\"")
                    })
                    .count()
            })
            .unwrap_or(0);
        if hits != 1 {
            failures.push(format!(
                "[{name}] {side}: expected exactly one ZodError-carrying skip warning, got {hits}: \
                 {warnings}"
            ));
        }
    }
}

// ── [P4.143 Tier 2 item 10] the refused chat create's repository lines ──────

/// The messages v4's refused chat create logs (its oracle's
/// `REPO_LOG_MESSAGES`): the THREE repository ERRORs and the per-chat WARN.
const REPO_LOG_MESSAGES: &[&str] = &[
    "Data validation failed",
    "Error creating entity",
    "Failed to create chat",
    "Failed to restore chat",
];

/// v5's captured lines (`LEVEL target message k=v …`, the `CaptureLayer`
/// shape) projected onto the oracle's `{level, message, collection, chatId,
/// error}` record. `error` is every line's LAST field, so it runs to the end
/// of the line (a ZodError message spans many).
fn v5_repo_logs(lines: &[String]) -> Vec<Value> {
    let mut out = Vec::new();
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let (Some(level), Some(_target), Some(rest)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(message) = REPO_LOG_MESSAGES
            .iter()
            .find(|m| rest.starts_with(&format!("{m} ")))
        else {
            continue;
        };
        let fields = &rest[message.len() + 1..];
        let mut rec = serde_json::Map::new();
        rec.insert("level".into(), json!(level.to_lowercase()));
        rec.insert("message".into(), json!(message));
        let (head, error) = fields.split_once("error=").unwrap_or((fields, ""));
        for kv in head.split_whitespace() {
            if let Some((k, v)) = kv.split_once('=') {
                rec.insert(k.to_string(), json!(v));
            }
        }
        rec.insert("error".into(), json!(error));
        out.push(Value::Object(rec));
    }
    out
}

/// Compare v5's refused-chat lines to v4's `repoLogs`, in order: level,
/// message, `collection` / `chatId`, and `error` byte for byte — or, on a
/// serde arm (`serde = Some((v4_path_key, v5_serde_prefix))`), the RECORDED
/// divergence: v4's `error` a Zod message whose first issue path is
/// `[v4_path_key]`, v5's starting with serde's sentence (VANISHED if they
/// agree). `strictFailures`: restore runs OUTSIDE the strict-repository scope
/// on both sides, so it is pinned on v4 to exactly the lines `strict_lines`
/// names (none here) and ABSENT on v5, then dropped from the compare.
fn compare_repo_logs(
    name: &str,
    want: &Value,
    got_lines: &[String],
    serde: Option<(&str, &str)>,
    strict_lines: &[&str],
    failures: &mut Vec<String>,
) {
    let Some(want) = want.as_array() else {
        failures.push(format!(
            "[{name}] the oracle carries no `repoLogs` — regenerate it"
        ));
        return;
    };
    if got_lines.iter().any(|l| l.contains("strictFailures=")) {
        failures.push(format!(
            "[{name}] v5 logs `strictFailures` on a restore — restore runs outside the strict scope on both sides"
        ));
    }
    let got = v5_repo_logs(got_lines);
    let mut want: Vec<Value> = want.clone();
    for w in want.iter_mut() {
        let message = w["message"].as_str().unwrap_or("").to_string();
        let strict = w.as_object_mut().and_then(|o| o.remove("strictFailures"));
        let expect = strict_lines
            .contains(&message.as_str())
            .then_some(json!(true));
        if strict != expect {
            failures.push(format!(
                "[{name}] v4 `strictFailures` on {message:?}: {strict:?}, recorded {expect:?}"
            ));
        }
    }
    if got.len() != want.len() {
        failures.push(format!(
            "[{name}] refused-chat repository lines: v5 {} vs v4 {}\n  rust:   {got:?}\n  oracle: \
             {want:?}",
            got.len(),
            want.len()
        ));
        return;
    }
    for (i, (mut g, mut w)) in got.into_iter().zip(want).enumerate() {
        if let Some((key, prefix)) = serde {
            let (ge, we) = (
                g["error"].as_str().unwrap_or("").to_string(),
                w["error"].as_str().unwrap_or("").to_string(),
            );
            let v4_first_path = serde_json::from_str::<Value>(&we)
                .ok()
                .and_then(|v| v.get(0).and_then(|i| i.get("path")).cloned());
            if ge == we {
                failures.push(format!(
                    "[{name}] line {i}: the serde-arm divergence VANISHED — both log {ge:?}; \
                     retire the pin"
                ));
            } else if !(is_zod_error_message(&we)
                && v4_first_path == Some(json!([key]))
                && ge.starts_with(prefix))
            {
                failures.push(format!(
                    "[{name}] line {i}: the serde-arm divergence has the WRONG SHAPE\n  rust:   \
                     {ge:?}\n  oracle: {we:?}"
                ));
            }
            g["error"] = json!("<SERDE-ARM-DIVERGENCE>");
            w["error"] = json!("<SERDE-ARM-DIVERGENCE>");
        }
        if g != w {
            failures.push(format!(
                "[{name}] refused-chat repository line {i} differs\n  rust:   {g}\n  oracle: {w}"
            ));
        }
    }
}

/// Is `tail` a `ZodError.message` — a non-empty JSON array of objects each
/// carrying Zod's `code`, `path` and `message` keys?
fn is_zod_error_message(tail: &str) -> bool {
    serde_json::from_str::<Value>(tail)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .is_some_and(|issues| {
            !issues.is_empty()
                && issues.iter().all(|i| {
                    i.get("code").is_some_and(Value::is_string)
                        && i.get("path").is_some_and(Value::is_array)
                        && i.get("message").is_some_and(Value::is_string)
                })
        })
}

/// [P4.158 R-G] The binary's three tests run one at a time: every case reads
/// the process-global capture below, and the acceptance test restores the same
/// archives — run beside the family, its lines landed in the family's buffer.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// A PROCESS-GLOBAL capture (the `search_replace_equivalence` precedent): the
/// restore's chat creates log inside `db.write`, on the WRITER thread, where a
/// thread-scoped subscriber sees nothing. Installed once per binary; the
/// comparator filters to [`REPO_LOG_MESSAGES`], so the binary's other test
/// logging into the same buffer cannot colour it.
fn global_capture() -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
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

/// [P4.143 item 2] **The restore's serde arm** — a RECORDED DIVERGENCE,
/// pinned in both directions (P4.130's un-planted follow-up).
/// `restore-archive-chat-serde-arm.zip` carries one chat, `c…0006` ("The
/// Serde Room"), whose `scenarioText` is the NUMBER 5 and whose Concierge
/// columns are valid. v4's `repos.chats.create` → `validate`
/// (`ChatMetadataBaseSchema`) throws the ZodError, so its skip warning's tail
/// is `[{"expected": "string", "code": "invalid_type", "path":
/// ["scenarioText"], …}]`; v5 checks only the Concierge columns before its
/// typed decode (`concierge_columns_zod_error`'s recorded scope), so the
/// decode refuses the number with serde's sentence and the restore closure in
/// `crates/quilltap-core/src/services/backup/restore/orchestrator.rs` (its
/// `skip` serves both refusals) pushes that instead. Both skip the chat; only
/// the tail differs. VANISHED if the two tails agree (retire the pin — the
/// generated schema-shape table, P4.143 Tier 3 item 12, closes it); WRONG
/// SHAPE unless v4's tail is a Zod message whose FIRST issue's path is
/// `["scenarioText"]` and v5's starts with serde's sentence. The warning is
/// then replaced with `<head><SERDE-ARM-DIVERGENCE>` on both sides, and the
/// rest of `summary.warnings` compares verbatim.
const SERDE_ROOM_CASE: &str = "restore_chat_serde_arm_replace";
const SERDE_ROOM_HEAD: &str = "Failed to restore chat \"The Serde Room\": ";
const SERDE_ROOM_V5_PREFIX: &str = "invalid type: integer `5`, expected a string";

fn classify_restore_serde_arm(
    name: &str,
    got: &Value,
    want: &Value,
    failures: &mut Vec<String>,
) -> (Value, Value) {
    let tail_of = |warnings: &Value| -> Option<String> {
        warnings
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find_map(|w| w.strip_prefix(SERDE_ROOM_HEAD).map(str::to_string))
    };
    let is_zod_scenario_text = |tail: &str| {
        serde_json::from_str::<Value>(tail)
            .ok()
            .and_then(|v| v.as_array().cloned())
            .is_some_and(|issues| {
                !issues.is_empty()
                    && issues.iter().all(|i| {
                        i.get("code").is_some_and(Value::is_string)
                            && i.get("path").is_some_and(Value::is_array)
                            && i.get("message").is_some_and(Value::is_string)
                    })
                    && issues[0]["path"] == json!(["scenarioText"])
            })
    };
    let (g, w) = (tail_of(got), tail_of(want));
    match (&g, &w) {
        (Some(g), Some(w)) if g == w => failures.push(format!(
            "[{name}] the restore serde-arm divergence VANISHED — both sides now say {g:?}; \
             retire the pin"
        )),
        (Some(g), Some(w)) if is_zod_scenario_text(w) && g.starts_with(SERDE_ROOM_V5_PREFIX) => {}
        _ => failures.push(format!(
            "[{name}] the restore serde-arm divergence has the WRONG SHAPE\n  rust:   {g:?}\n  \
             oracle: {w:?}"
        )),
    }
    let carve = |warnings: &Value| -> Value {
        let mut out = warnings.clone();
        if let Some(ws) = out.as_array_mut() {
            for w in ws.iter_mut() {
                if w.as_str().is_some_and(|s| s.starts_with(SERDE_ROOM_HEAD)) {
                    *w = json!(format!("{SERDE_ROOM_HEAD}<SERDE-ARM-DIVERGENCE>"));
                }
            }
        }
        out
    };
    (carve(got), carve(want))
}

/// [P4.143 item 2] The serde-arm plant pinned by NAME, so the arm cannot go
/// vacuous: chat `c…0006` is ABSENT on both sides, and both warning lists
/// carry exactly one `Failed to restore chat "The Serde Room": ` warning.
fn assert_serde_room_skipped(
    name: &str,
    got_summary: &RestoreSummary,
    want_summary: &Value,
    got_state: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want_state: &Value,
    failures: &mut Vec<String>,
) {
    if name != SERDE_ROOM_CASE {
        return;
    }
    const SERDE_ROOM: &str = "c1000000-0000-4000-8000-000000000006";
    let got_chats = got_state
        .get("main")
        .and_then(|m| m.get("chats"))
        .map(|rows| rows.iter().any(|r| r["id"] == SERDE_ROOM));
    let want_chats = want_state["main"]["chats"]
        .as_array()
        .map(|rows| rows.iter().any(|r| r["id"] == SERDE_ROOM));
    if got_chats != Some(false) || want_chats != Some(false) {
        failures.push(format!(
            "[{name}] the serde-arm chat must be ABSENT on both sides (v5 present: \
             {got_chats:?}, v4 present: {want_chats:?})"
        ));
    }
    let got_warn = json!(got_summary.warnings);
    for (side, warnings) in [("v5", &got_warn), ("v4", &want_summary["warnings"])] {
        let hits = warnings
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .filter(|w| w.starts_with(SERDE_ROOM_HEAD))
                    .count()
            })
            .unwrap_or(0);
        if hits != 1 {
            failures.push(format!(
                "[{name}] {side}: expected exactly one serde-arm skip warning, got {hits}: \
                 {warnings}"
            ));
        }
    }
}

/// [P4.106 item 6] **The pre-4.10-archive arm.** Every committed restore
/// archive predates Inform (`e7d77bb60`): none carries `data/chat-informs.json`
/// (measured over all fifteen zips). A restore of one must succeed (the loop's
/// `expect` is the no-error half), leave the target's `chat_informs` table
/// PRESENT (both apps provision it) and EMPTY on both sides, and report
/// `chatInforms: 0` in both summaries. The whole-table diff already compares
/// the empty table; this pins it by name so the arm cannot be vacuous — an
/// absent table on either side reddens here rather than passing as "no rows".
fn assert_pre_410_archive_restores_no_informs(
    name: &str,
    summary: &RestoreSummary,
    want_summary: &Value,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    // [P4.147 item 9] the archives that DO carry informs — P4.147's, and
    // P4.161's whole-row refusal derivation (`assert_refusals_restored`).
    if name == INFORMS_CASE || name == "restore_inform_refusals_replace" {
        return;
    }
    match got.get("main").and_then(|t| t.get("chat_informs")) {
        None => failures.push(format!(
            "[{name}] v5's restored target has no chat_informs table"
        )),
        Some(rows) if !rows.is_empty() => failures.push(format!(
            "[{name}] a pre-4.10 archive restored {} chat_informs row(s) on v5",
            rows.len()
        )),
        Some(_) => {}
    }
    match want["main"]["chat_informs"].as_array() {
        None => failures.push(format!(
            "[{name}] v4's restored target has no chat_informs table"
        )),
        Some(rows) if !rows.is_empty() => failures.push(format!(
            "[{name}] a pre-4.10 archive restored {} chat_informs row(s) on v4",
            rows.len()
        )),
        Some(_) => {}
    }
    let got_n = serde_json::to_value(summary).expect("summary")["chatInforms"].clone();
    if got_n != 0 || want_summary["chatInforms"] != 0 {
        failures.push(format!(
            "[{name}] summary chatInforms: v5 {got_n}, v4 {} (both must be 0)",
            want_summary["chatInforms"]
        ));
    }
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

/// ## [P4.147] The #141 + #142 acceptance measure, on v5 ALONE
///
/// The oracle family above proves v5 diverges from v4 exactly as ruled. This
/// proves the RESULT is what dogfood #141/#142 asked for, against the archive
/// itself — no oracle, so it runs unconditionally and can never SKIP (the
/// `preview_writes_nothing` precedent). A `replace` restore into a FRESHLY
/// provisioned target (the disaster-recovery shape: every built-in pointer
/// names the target's own, wiped store) must leave:
///
/// 1. exactly the archive's stores — `COUNT(doc_mount_points)` equal to the
///    archive's, no store minted for an entity the archive carries a store for;
/// 2. no duplicate store names beyond the archive's own;
/// 3. every character / project / group on the store the ARCHIVE names, with
///    the archive's link count behind it (Lorian 11, Riya 9, project 4, group 4
///    on `restore-archive.zip` — the "Friday on her 805-link vault" analog);
/// 4. (an archive carrying its own Uploads store) ZERO `Quilltap Uploads mount
///    has not been provisioned` warnings, and every restored project-less file
///    on `mount-blob:<the archive's Uploads id>:…`;
/// 5. `userUploadsMountPointId` naming the archive's store, and that store
///    existing;
/// 6. no store unreferenced by an entity pointer, a built-in pointer or an
///    archive id.
///
/// Plus Tier 2 item 14: a target-side archived-character bundle (`files`, category
/// `ARCHIVE`, kept by the default wipe) named like a restored file — the two
/// must not collide: the bundle survives untouched, the restored file lands in
/// the archive's Uploads store at its own unique path.
#[test]
fn a_replace_restore_lands_every_entity_on_the_archives_stores() {
    let _serial = serial();
    const KEPT_BUNDLE: &str = "a8000000-0000-4000-8000-000000000001";
    for archive in ["restore-archive.zip", "restore-archive-uploads.zip"] {
        let scratch = fresh_scratch(&format!("acceptance-{archive}"));
        let instance = scratch.root.join("instance");
        let db = fresh_instance(&instance);
        let host = TestHost {
            root: scratch.root.clone(),
        };
        std::fs::create_dir_all(host.temp_dir()).unwrap();
        let zip = archives_dir().join(archive);
        let carries_uploads = archive == "restore-archive-uploads.zip";
        drop(db);
        if carries_uploads {
            let w = quilltap_core::db::Writer::open_writable(
                &instance.join("quilltap.db"),
                TEST_PEPPER,
            )
            .unwrap();
            quilltap_core::db::files::FilesRepository::new(w.connection())
                .create(
                    &quilltap_core::db::files::FileCreate {
                        user_id: SINGLE_USER_ID.to_string(),
                        sha256: "a".repeat(64),
                        original_filename: "portrait.png".into(),
                        mime_type: "application/octet-stream".into(),
                        size: 3.0,
                        width: None,
                        height: None,
                        is_plain_text: None,
                        linked_to: vec![],
                        source: "UPLOADED".into(),
                        category: "ARCHIVE".into(),
                        generation_prompt: None,
                        generation_model: None,
                        generation_revised_prompt: None,
                        generation_key: None,
                        description: None,
                        tags: vec![],
                        project_id: None,
                        folder_path: None,
                        storage_key: Some(format!("mount-blob:{}:{}", "0".repeat(36), KEPT_BUNDLE)),
                        file_status: "ok".into(),
                    },
                    &quilltap_core::db::files::CreateOptions {
                        id: KEPT_BUNDLE.into(),
                        created_at: "2026-10-05T00:00:00.000Z".into(),
                        updated_at: "2026-10-05T00:00:00.000Z".into(),
                    },
                )
                .expect("seed the kept bundle");
        }
        let db = reopen_instance(&instance);
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
                Default::default(),
            ))
            .expect("restore succeeded");
        drop(db);
        let got = serde_json::to_value(read_state(&instance)).unwrap();
        let a = archive_stores(&zip, &host.temp_dir());
        let ctx = |msg: String| format!("[{archive}] {msg}");

        // 1 + 2.
        let points = rows_of(&got, "mountIndex", "doc_mount_points");
        assert_eq!(
            points.len(),
            a.point_ids.len(),
            "{}",
            ctx("store count must equal the archive's".into())
        );
        let mut names: BTreeMap<String, usize> = BTreeMap::new();
        for p in points {
            *names
                .entry(str_at(p, "name").unwrap_or("").trim().to_lowercase())
                .or_insert(0) += 1;
        }
        let mut archive_names: BTreeMap<String, usize> = BTreeMap::new();
        for n in &a.point_names {
            *archive_names.entry(n.trim().to_lowercase()).or_insert(0) += 1;
        }
        assert_eq!(names, archive_names, "{}", ctx("store names".into()));

        // 3.
        let mut checked = 0;
        for (table, column, id, archived) in &a.pointers {
            let Some(archived) = archived.as_ref().filter(|p| a.point_ids.contains(*p)) else {
                continue;
            };
            let pointer = pointer_on(&got, table, column, id).flatten();
            assert_eq!(
                pointer.as_deref(),
                Some(archived.as_str()),
                "{}",
                ctx(format!("{table} {id} must point at the archive's store"))
            );
            let links = rows_of(&got, "mountIndex", "doc_mount_file_links")
                .iter()
                .filter(|l| str_at(l, "mountPointId") == Some(archived.as_str()))
                .count();
            assert_eq!(
                links,
                a.links_per_point.get(archived).copied().unwrap_or(0),
                "{}",
                ctx(format!(
                    "{table} {id}'s store must carry the archive's links"
                ))
            );
            checked += 1;
        }
        assert_eq!(checked, 4, "{}", ctx("all four entities checked".into()));
        if archive == "restore-archive.zip" {
            let count = |mp: &str| a.links_per_point.get(mp).copied().unwrap_or(0);
            assert_eq!(
                (
                    count("aff3114e-ed90-4d5b-99c1-ef3fa20203fc"),
                    count("f6b22d51-85e5-4bee-a053-262dd965962f"),
                    count("5c17e916-5f79-4cca-a134-ec09c05924e9"),
                    count("60a8194d-f8ea-4540-af77-5e13e7ed0e9b"),
                ),
                (11, 9, 4, 4),
                "the survey's measured link counts"
            );
        }

        // 4 + 5 (+ item 14).
        let settings = |k: &str| {
            rows_of(&got, "main", "instance_settings")
                .iter()
                .find(|r| str_at(r, "key") == Some(k))
                .and_then(|r| str_at(r, "value").map(str::to_string))
        };
        if carries_uploads {
            let uploads = a.settings["userUploadsMountPointId"].clone();
            assert!(
                summary
                    .warnings
                    .iter()
                    .all(|w| !w.contains(UPLOADS_UNPROVISIONED)),
                "{}",
                ctx(format!("no Uploads warning: {:?}", summary.warnings))
            );
            assert_eq!(settings("userUploadsMountPointId"), Some(uploads.clone()));
            assert!(point_exists(&got, &uploads));
            let files = rows_of(&got, "main", "files");
            let restored: Vec<&Value> = files
                .iter()
                .filter(|f| str_at(f, "id") != Some(KEPT_BUNDLE) && f["projectId"].is_null())
                .collect();
            assert_eq!(
                restored.len(),
                2,
                "{}",
                ctx("both project-less files restored".into())
            );
            for f in restored {
                assert!(
                    str_at(f, "storageKey")
                        .is_some_and(|k| k.starts_with(&format!("mount-blob:{uploads}:"))),
                    "{}",
                    ctx(format!("{f} must live on the archive's Uploads store"))
                );
            }
            let kept = files
                .iter()
                .find(|f| str_at(f, "id") == Some(KEPT_BUNDLE))
                .expect("the kept bundle survives the wipe");
            assert_eq!(str_at(kept, "category"), Some("ARCHIVE"));
            let uploads_paths: Vec<String> = rows_of(&got, "mountIndex", "doc_mount_file_links")
                .iter()
                .filter(|l| str_at(l, "mountPointId") == Some(uploads.as_str()))
                .filter_map(|l| str_at(l, "relativePath").map(str::to_lowercase))
                .collect();
            let unique: HashSet<&String> = uploads_paths.iter().collect();
            assert_eq!(
                unique.len(),
                uploads_paths.len(),
                "{}",
                ctx("Uploads paths unique".into())
            );
            assert!(
                uploads_paths.iter().any(|p| p == "restored/portrait.png"),
                "{}",
                ctx(format!(
                    "the restored portrait.png must be linked: {uploads_paths:?}"
                ))
            );
            assert!(
                !rows_of(&got, "mountIndex", "doc_mount_blobs")
                    .iter()
                    .any(|b| str_at(b, "id") == Some(KEPT_BUNDLE)),
                "{}",
                ctx("nothing in the restored store claims the kept bundle's blob id".into())
            );
        }

        // 6.
        let mut referenced: HashSet<String> = a.point_ids.clone();
        for (table, column, id, _) in &a.pointers {
            if let Some(Some(p)) = pointer_on(&got, table, column, id) {
                referenced.insert(p);
            }
        }
        for key in quilltap_core::services::backup::uuid_remap::MOUNT_POINT_SETTING_KEYS {
            if let Some(v) = settings(key) {
                referenced.insert(v);
            }
        }
        for p in points {
            let id = str_at(p, "id").unwrap_or("");
            assert!(
                referenced.contains(id),
                "{}",
                ctx(format!("store {id} is referenced by nothing — an orphan"))
            );
        }
        println!(
            "OK acceptance {archive}: {} stores, {checked} entities on the archive's stores",
            points.len()
        );
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
    let _serial = serial();
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
            Default::default(),
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
/// ## The one masked substring — MASK RETIRED (P4.50, 2026-08-19)
///
/// v5's warning used to read `Failed to restore file "portrait.png": key
/// derivation failed: Quilltap Uploads mount has not been provisioned` where
/// v4's read `…: Quilltap Uploads mount has not been provisioned`. The extra
/// clause was never a different failure — it was `DbError::Key`'s Display prefix
/// leaking into user-visible text, from 244 call sites that used the variant as
/// a general-purpose message carrier while its `Display` claimed a cipher fault.
/// This file carried a `LEAKED_PREFIX` strip so the rest of the sentence could be
/// compared verbatim, and recorded that a future fix would need no change here.
///
/// P4.50 landed that fix (`DbError::Internal`, whose `Display` is the bare
/// message — dogfood finding #96). The strip is therefore gone rather than left
/// as a no-op: with it removed these warnings byte-compare against v4's whole
/// sentence, which is strictly stronger, and any regrowth of the prefix onto a
/// user-visible restore warning reds this family instead of being absorbed.
fn compare_warnings(name: &str, got: &Value, want: &Value, failures: &mut Vec<String>) {
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let (g, w) = (
        files_phase_last(strings(got), |s: &String| is_files_phase_warning(s)),
        files_phase_last(strings(want), |s: &String| is_files_phase_warning(s)),
    );
    if g != w {
        failures.push(format!(
            "[{name}] summary.warnings differ\n  rust:   {g:?}\n  oracle: {w:?}"
        ));
    }
}

/// ## [P4.158 R-G] The ruled files-phase slot, in the warnings and the census
///
/// v4 runs its files phase at `22a-bis` (after 22a, before 22b); v5 after the
/// WHOLE doc-store family — RULED 2026-07-26, see [`PHASE_ORDER_RESIDUAL`]. So
/// on a case where both the files phase and a 22b–22h phase warn (first seen
/// on `restore_phase_warns_replace`), the files phase's entries sit at
/// different points. They are compared as their own subsequence (moved to the
/// end, order kept, on both sides); every other entry keeps its order.
fn files_phase_last<T>(items: Vec<T>, is_files_phase: impl Fn(&T) -> bool) -> Vec<T> {
    let (files, rest): (Vec<T>, Vec<T>) = items.into_iter().partition(|i| is_files_phase(i));
    rest.into_iter().chain(files).collect()
}

fn is_files_phase_warning(w: &str) -> bool {
    w.starts_with("Failed to restore file \"") || w.starts_with("File not found in backup: ")
}

/// ## [P4.D152] bug 117 — `files.sha256` must name the bytes actually stored
///
/// **The normalized table diff is BLIND to this column.** A `files` row whose
/// composites had to be normalized has EVERY `*sha256` in it replaced with
/// `<sha:derived-from-normalized>` — which is exactly the column bug 117 is
/// about. Measured, not assumed: restoring bug 117's own ordering (the replay
/// arm taking the archive's `sha256`) left the whole table diff GREEN while the
/// restored row carried the archive's lie.
///
/// So compare it here, before normalization, as plain strings. That is sound for
/// THIS archive precisely because neither of its two files can transcode
/// differently on the two engines: `portrait.png` is text-shaped, so no codec
/// decodes it and both bridges store the input bytes; `plate.png` is carried
/// verbatim from the archive and never re-ingested. Both engines must therefore
/// land the same hash — and the archive's own `files.sha256` is a fixed sentinel
/// that is neither of them, so a restore that copies the archive's value is
/// visible in both directions.
/// [P4.D158] v4 `2edd823c0`'s four arms — the additions that ride INSIDE an
/// existing column, asserted **within-tree** on v5's own restored cells.
///
/// The cross-side dump above already compares these cells to v4's, which catches
/// a one-sided loss. It cannot catch a two-sided one: if v5 dropped
/// `allowCheapFallback` and v4 dropped it too, the row diff would be green and
/// the port would still be wrong. So each of the four is also asserted against
/// the value the ARCHIVE carries, by name, so the failure says which addition
/// went missing rather than "a row differs".
///
/// v4's own framing, from the commit that motivated them: *a new column
/// announces itself with a migration; a new key in a JSON bag or a widened enum
/// domain is invisible to every schema check.*
fn assert_bag_keys_survive(
    name: &str,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    failures: &mut Vec<String>,
) {
    if !name.starts_with("restore_bag_keys") {
        return;
    }
    let empty: Vec<Value> = Vec::new();
    let table =
        |t: &str| -> &Vec<Value> { got.get("main").and_then(|p| p.get(t)).unwrap_or(&empty) };
    let mut fail = |arm: &str, msg: String| {
        failures.push(format!("[{name}] BAG-KEY ARM {arm}: {msg}"));
    };

    // ── arm 1: the widened enum domain, on both chats ───────────────────────
    //
    // Two rows, because a NARROWING and a DROP are different bugs: narrowing
    // 'UNCENSORED' to 'OFF' moves only the first cell (and would silently re-arm
    // the classifier on a chat the operator had ruled on); dropping it moves
    // both. The chats are identified by title, since `new-account` remaps every
    // id. P4.D227 (v4 `3b463d6b1`, #76): `chats.conciergeOverride` is DROPPED,
    // so the archive's legacy value now survives as the STATE restore derives
    // from it (P4.D226's translation: `UNCENSORED` → Unmoderated, `OFF` →
    // Locked), and the dropped column must not come back on the row.
    let chats = table("chats");
    let by_title = |t: &str| -> Option<&Value> {
        chats
            .iter()
            .find(|r| r.get("title").and_then(Value::as_str) == Some(t))
    };
    for (title, archived, want) in [
        ("A Lesson in Lift", "UNCENSORED", "unmoderated"),
        ("Quiet Interlude", "OFF", "locked"),
    ] {
        match by_title(title) {
            None => fail(
                "1 conciergeMode",
                format!("no restored chat titled {title:?}"),
            ),
            Some(row) => {
                let got_v = row.get("conciergeMode");
                if got_v.and_then(Value::as_str) != Some(want) {
                    fail(
                        "1 conciergeMode",
                        format!(
                            "chat {title:?} restored as {} — the archive's {archived:?} \
                             derives {want:?}",
                            serde_json::to_string(&got_v).unwrap()
                        ),
                    );
                }
                if row.get("conciergeOverride").is_some() {
                    fail(
                        "1 conciergeOverride",
                        format!("chat {title:?} carries the DROPPED column"),
                    );
                }
            }
        }
    }

    // ── arm 2: a JSON-bag key whose schema default is the opposite ──────────
    //
    // `allowCheapFallback` defaults to false, so losing it in transit reads as
    // the operator having DECLINED a stand-in they opted into. In `new-account`
    // mode the target keeps its own freshly-provisioned settings row too, so the
    // arm looks for the RESTORED one: the row whose bag carries the key at all.
    let settings = table("chat_settings");
    let restored_bag = settings.iter().filter_map(|r| {
        let raw = r.get("cheapLLMSettings")?.as_str()?;
        let bag: Value = serde_json::from_str(raw).ok()?;
        bag.get("embeddingProvider")
            .and_then(Value::as_str)
            .filter(|p| *p == "OPENAI")
            .filter(|_| bag.get("fallbackToLocal") == Some(&Value::Bool(false)))?;
        Some(bag)
    });
    let bags: Vec<Value> = restored_bag.collect();
    match bags.len() {
        1 => {
            if bags[0].get("allowCheapFallback") != Some(&Value::Bool(true)) {
                fail(
                    "2 allowCheapFallback",
                    format!(
                        "restored cheapLLMSettings is {} — the archive carries true, and the \
                         schema default (false) reads as a declined stand-in",
                        serde_json::to_string(&bags[0]).unwrap()
                    ),
                );
            }
        }
        n => fail(
            "2 allowCheapFallback",
            format!("expected exactly one RESTORED chat_settings bag, found {n}"),
        ),
    }

    // ── arm 3: a reserved key in an unvalidated bag ─────────────────────────
    //
    // Nothing downstream validates `image_profiles.parameters`, so nothing
    // downstream would notice `loras` going missing. The pre-existing `steps` is
    // asserted beside it so a bag-level REPLACEMENT and a key-level DROP are
    // different failures.
    let profiles = table("image_profiles");
    match profiles.len() {
        1 => {
            let raw = profiles[0].get("parameters").and_then(Value::as_str);
            let bag: Option<Value> = raw.and_then(|r| serde_json::from_str(r).ok());
            match bag {
                None => fail(
                    "3 parameters.loras",
                    format!("parameters is not a JSON object: {raw:?}"),
                ),
                Some(bag) => {
                    let loras = bag.get("loras").and_then(Value::as_array);
                    let ok = loras.map(|a| {
                        a.len() == 1
                            && a[0].get("source").and_then(Value::as_str)
                                == Some("author/some-lora")
                            && a[0].get("triggerPhrase").and_then(Value::as_str)
                                == Some("in the style of")
                    }) == Some(true);
                    if !ok {
                        fail(
                            "3 parameters.loras",
                            format!(
                                "restored parameters is {bag} — the archive carries one adapter"
                            ),
                        );
                    }
                    if bag.get("steps").and_then(Value::as_i64) != Some(30) {
                        fail(
                            "3 parameters.loras",
                            format!("the sibling `steps` key is gone too ({bag}) — that is a bag-level replacement, not a lost key"),
                        );
                    }
                }
            }
        }
        n => fail(
            "3 parameters.loras",
            format!("expected 1 image profile, found {n}"),
        ),
    }

    // ── arm 4: the row written by RAW SQL rather than a repository ──────────
    //
    // `instance_settings` is upserted directly (`restore.ts:879`), so the value
    // travels as an opaque string: the guard is that the row is written at all,
    // and written verbatim.
    let rows = table("instance_settings");
    match rows
        .iter()
        .find(|r| r.get("key").and_then(Value::as_str) == Some("memoryRecall"))
    {
        None => fail(
            "4 memoryRecall",
            format!(
                "no memoryRecall row — the archive carries one, and {} others restored",
                rows.len()
            ),
        ),
        Some(row) => {
            let raw = row.get("value").and_then(Value::as_str).unwrap_or_default();
            let bag: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
            if bag.get("perTurnConversationSummaries") != Some(&Value::Bool(true)) {
                fail(
                    "4 memoryRecall",
                    format!("restored value is {raw:?} — perTurnConversationSummaries is gone"),
                );
            }
        }
    }
}

fn assert_bug117_stored_sha(
    name: &str,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    if name != "restore_bug117_new_account" {
        return;
    }
    /// The archive's sentinel — see `build-restore-archive-bug117.test.ts`.
    const LIE: &str = "b117b117b117b117b117b117b117b117b117b117b117b117b117b117b117b117";

    let by_name = |rows: &[Value]| -> BTreeMap<String, String> {
        rows.iter()
            .filter_map(|r| {
                Some((
                    r.get("originalFilename")?.as_str()?.to_string(),
                    r.get("sha256")?.as_str()?.to_string(),
                ))
            })
            .collect()
    };
    let empty: Vec<Value> = Vec::new();
    let v5 = by_name(
        got.get("main")
            .and_then(|p| p.get("files"))
            .unwrap_or(&empty),
    );
    let v4 = by_name(
        &want["main"]["files"]
            .as_array()
            .cloned()
            .unwrap_or_default(),
    );

    for file in ["portrait.png", "plate.png"] {
        match (v5.get(file), v4.get(file)) {
            (Some(g), Some(w)) => {
                if g != w {
                    failures.push(format!(
                        "[{name}] bug 117: {file} sha256 rust {g} vs oracle {w}"
                    ));
                }
                if g == LIE {
                    failures.push(format!(
                        "[{name}] bug 117: {file} kept the ARCHIVE's pre-transcode hash — the restored row names bytes that exist nowhere"
                    ));
                }
            }
            _ => failures.push(format!(
                "[{name}] bug 117: {file} is missing from one side's restored `files` — the arm has gone stale (v5={:?} v4={:?})",
                v5.get(file),
                v4.get(file)
            )),
        }
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

/// The phase-order divergence [`REPLAY_DEDUPE`] pins after bug 12, asserted in
/// BOTH directions.
///
/// **What CONVERGED (bug 12, v4 `3bb664f0`).** v4 adopted v5's
/// `carried_store_rows` skip check, so it no longer re-ingests a carried file: the
/// storageKeys now AGREE, and the per-carried-file blob/file/link re-ingest — the
/// whole basis of the old carve-out — is gone. gen-2's archive converges
/// completely and is a plain equality now; only the two archives whose file phase
/// still COLLIDES stay here (uploads, compact).
///
/// **What PERSISTS (the RULED phase-order divergence — v5 KEEPS its later slot).**
/// v4 kept its `22a-bis` file phase, so on an archive with a legacy-disk-key file
/// re-ingested into `restored/`, v4's replay wins the race into that folder and
/// the doc-store folder phase then collides:
///   - **uploads**: v4 emits `Failed to restore doc-store folder "restored":
///     UNIQUE constraint failed` and restores one FEWER `doc_mount_folders` row;
///     v5 (later slot) restores the archive's whole tree cleanly.
///   - **compact**: additionally, v4 cannot dedup the archive's >3 MB (multi-chunk)
///     carried file — the sparse-array export boundary makes its skip check miss —
///     so v4 invents a PHANTOM doc-store copy (one extra blob + file + link, in a
///     store the archive never linked it into). v5 restores exactly the one atlas
///     file the archive carries. Confirmed by measurement (P4.D51): the archive's
///     `doc-mount-file-links.json` names `uploads/atlas-plates.bin` ONCE, in the
///     Uploads mount; v4 also lands an `atlas-plates.bin` in "Project Files: The
///     Voyage", which the archive never references.
///
/// Both are v5-ahead under the standing 2026-08-03 backup/restore ruling ("v5
/// FIXES v4's bugs in this family"). Asserted in both directions:
///   - v5 is CLEAN — every carried file resolves, no collision / refusal warning;
///   - v4 STILL diverges — a collision warning, fewer folders, or a phantom blob;
///   - v5 restores AT LEAST as many folders/links as v4, and v4 holds AT LEAST as
///     many blobs as v5 (the phantom).
///
/// If v4 fully converges — no warning and every carved table row count agrees —
/// the retire tripwire fires: move the case to a plain / order-insensitive
/// equality (the gen-2 shape).
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
    let got_keys = storage_keys_by_name(got_files);
    let got_blob_ids: HashSet<String> = got
        .get("mountIndex")
        .and_then(|p| p.get("doc_mount_blobs"))
        .unwrap_or(&empty)
        .iter()
        .filter_map(|b| b.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();

    // A. v5 kept the archive's store rows and every carried file RESOLVES to a
    //    present blob — the failure this surface fears most is silently dropping a
    //    file the user expected back. (The storageKey VALUES are not compared
    //    across engines: `new_account` mode remaps every mount id, so the keys
    //    differ mechanically there even when the content is identical.)
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
    }

    // B. Warnings: v5 is CLEAN (no phase-order collision or link refusal); v4 is
    //    where the divergence surfaces.
    const REFUSED_LINK: &str = "Failed to restore doc-store file link";
    const FOLDER_COLLISION: &str = "Failed to restore doc-store folder";
    let is_divergence_warning =
        |s: &str| s.starts_with(REFUSED_LINK) || s.starts_with(FOLDER_COLLISION);
    let want_warn: Vec<String> = want_summary["warnings"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if summary.warnings.iter().any(|s| is_divergence_warning(s)) {
        failures.push(format!(
            "[{name}] v5 emitted a phase-order collision / refusal warning ({:?}) — its later \
             file-phase slot is supposed to restore the archive's tree cleanly",
            summary.warnings
        ));
    }

    // C. The divergence must still be LIVE — else the carve-out is masking a
    //    convergence. v4's 22a-bis slot diverges from v5's later one in one of two
    //    ways: it warns about the `restored/` folder collision (uploads), or it
    //    ends with a different row count on a carved store table (compact's >3 MB
    //    phantom gives it MORE; the folder collision gives it FEWER). If NEITHER
    //    holds, v4 has fully converged and the case should move to a plain /
    //    order-insensitive equality (the gen-2 shape).
    let count = |src: &BTreeMap<String, BTreeMap<String, Vec<Value>>>, table: &str| -> i64 {
        src.get("mountIndex")
            .and_then(|p| p.get(table))
            .map(Vec::len)
            .unwrap_or(0) as i64
    };
    let want_count = |table: &str| -> i64 {
        want["mountIndex"][table]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0) as i64
    };
    let v4_warned = want_warn.iter().any(|s| is_divergence_warning(s));
    let counts_differ = ["doc_mount_blobs", "doc_mount_files", "doc_mount_file_links"]
        .iter()
        .any(|t| count(got, t) != want_count(t));
    if !v4_warned && !counts_differ {
        failures.push(format!(
            "[{name}] v4's restore now agrees with v5's on every carved store-table row count and \
             emits no collision / refusal warning — v4 has FULLY CONVERGED (it adopted v5's later \
             file-phase slot and the >3 MB dedup). Retire this case from REPLAY_DEDUPE and \
             compare its tables for equality (order-insensitively, like PHASE_ORDER_RESIDUAL)."
        ));
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

// ─────────────────────────────────────────────────────────────────────────────
// The #58 orphaned-rows divergence (P4.28 + this round's unification wire)
// ─────────────────────────────────────────────────────────────────────────────

/// ## ⚠ THE RULED ORPHAN-SKIP DIVERGENCE (dogfood #58, 2026-08-03)
///
/// `restore-archive-orphan-links.zip` carries 9 `doc_mount_file_links` rows,
/// 7 `doc_mount_folders` rows and 4 chunks whose `doc_mount_points` parent is
/// NOT in the archive (a store deleted without its children, dumped verbatim by
/// backup's raw `SELECT *`). Under the standing 2026-08-03 backup/restore
/// ruling v5 SKIPS each one with a sentence naming what is missing, while v4 on
/// this family's FK-less generateDDL target inserts every orphan silently.
/// Asserted in both directions: the v5 side must land ZERO orphans and the
/// oracle side must land EXACTLY the 9/7/4 — so the moment v4 grows its own
/// orphan handling this fails with a retire-the-divergence instruction. The
/// healthy remainder of all three tables is still diffed row for row.
const ORPHAN_LINKS_CASE: &str = "restore_orphan_links_replace";
const ORPHAN_TABLES: &[&str] = &[
    "doc_mount_file_links",
    "doc_mount_folders",
    "doc_mount_chunks",
];
/// (links, folders, chunks) the committed archive carries orphaned — pinned by
/// the builder (`build-restore-archive-orphan-links.test.ts`, victim store
/// deleted BY NAME) and by `restore_vintage_state`'s own arms.
const ORPHAN_COUNTS: (usize, usize, usize) = (9, 7, 4);

fn orphan_ids(rows: &[Value], key: &str, parents: &HashSet<String>) -> Vec<String> {
    rows.iter()
        .filter(|r| {
            r.get(key)
                .and_then(Value::as_str)
                .is_some_and(|v| !parents.contains(v))
        })
        .map(|r| {
            r.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

fn table_ids(rows: &[Value]) -> HashSet<String> {
    rows.iter()
        .filter_map(|r| r.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

/// The (parent key, expected oracle-side orphan count) for one of the three
/// tables, plus the parent id set for each side. Chunks hang off links, not
/// points, so their parent set is the same side's LINK ids.
#[allow(clippy::too_many_arguments)]
fn assert_orphan_divergence(
    name: &str,
    table: &str,
    got_rows: &[Value],
    want_rows: &[Value],
    got_points: &HashSet<String>,
    want_points: &HashSet<String>,
    got_links: &HashSet<String>,
    want_links: &HashSet<String>,
    literals: &HashSet<String>,
    shas_got: &HashSet<String>,
    shas_want: &HashSet<String>,
) -> Vec<String> {
    let mut failures = Vec::new();
    let (parent_key, got_parents, want_parents, want_expected) = match table {
        "doc_mount_file_links" => ("mountPointId", got_points, want_points, ORPHAN_COUNTS.0),
        "doc_mount_folders" => ("mountPointId", got_points, want_points, ORPHAN_COUNTS.1),
        "doc_mount_chunks" => ("linkId", got_links, want_links, ORPHAN_COUNTS.2),
        other => panic!("assert_orphan_divergence: unexpected table {other}"),
    };

    let got_orphans = orphan_ids(got_rows, parent_key, got_parents);
    if !got_orphans.is_empty() {
        failures.push(format!(
            "[{name}] mountIndex.{table}: v5 landed {} orphaned row(s) — the #58 skip check \
             was reverted or bypassed ({:?})",
            got_orphans.len(),
            got_orphans
        ));
    }
    let want_orphans = orphan_ids(want_rows, parent_key, want_parents);
    if want_orphans.len() != want_expected {
        failures.push(format!(
            "[{name}] mountIndex.{table}: the oracle landed {} orphaned row(s) where the \
             committed archive carries {want_expected} — if v4 has stopped inserting orphans \
             it has adopted its own #58 handling and this divergence must be RE-RULED (retire \
             the ORPHAN_LINKS_CASE arm and let the tables diff normally); if the count merely \
             moved, the archive was rebuilt and these pins must move with it.",
            want_orphans.len()
        ));
    }

    // The healthy remainder must still agree row for row, under the same
    // per-table labelling the main path uses — the divergence is EXACTLY the
    // orphans, nothing else.
    let healthy = |rows: &[Value], parents: &HashSet<String>| -> Vec<Value> {
        rows.iter()
            .filter(|r| {
                r.get(parent_key)
                    .and_then(Value::as_str)
                    .is_some_and(|v| parents.contains(v))
            })
            .cloned()
            .collect()
    };
    let mut n_got = Normalizer::new(literals.clone(), shas_got.clone());
    let mut n_want = Normalizer::new(literals.clone(), shas_want.clone());
    let g = n_got.value(&Value::Array(healthy(got_rows, got_parents)));
    let w = n_want.value(&Value::Array(healthy(want_rows, want_parents)));
    if g != w {
        failures.push(format!(
            "[{name}] mountIndex.{table}: the HEALTHY rows diverged — the #58 carve-out only \
             covers the orphans\n  rust:   {g}\n  oracle: {w}"
        ));
    }
    failures
}

/// v5's warnings for the orphan case are v4's plus exactly the skip sentences;
/// v4 must have none of them. Both directions, like the table half.
fn assert_orphan_warnings(name: &str, gv: &Value, wv: &Value, failures: &mut Vec<String>) {
    let arr = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let got = arr(gv);
    let want = arr(wv);
    let is_skip = |w: &String| w.starts_with("Skipped doc-store ");

    let want_skips = want.iter().filter(|w| is_skip(w)).count();
    if want_skips != 0 {
        failures.push(format!(
            "[{name}] warnings: the oracle carries {want_skips} \"Skipped doc-store\" \
             sentence(s) — v4 has adopted the #58 skip check; re-rule the divergence."
        ));
    }

    let (links, folders, chunks) = ORPHAN_COUNTS;
    let count = |suffix: &str, prefix: &str| {
        got.iter()
            .filter(|w| w.starts_with(prefix) && w.ends_with(suffix))
            .count()
    };
    let link_skips = count(
        "its document store is not in the backup",
        "Skipped doc-store file link \"",
    );
    let folder_skips = count(
        "its document store is not in the backup",
        "Skipped doc-store folder \"",
    );
    let chunk_skips = got
        .iter()
        .filter(|w| *w == "Skipped doc-store chunk: its file link is not in the backup")
        .count();
    if (link_skips, folder_skips, chunk_skips) != (links, folders, chunks) {
        failures.push(format!(
            "[{name}] warnings: expected {links}/{folders}/{chunks} link/folder/chunk skip \
             sentences, got {link_skips}/{folder_skips}/{chunk_skips}"
        ));
    }

    // Minus the skips, the two sides must agree (as multisets — order within a
    // phase is stable but the skips interleave).
    let mut got_rest: Vec<&String> = got.iter().filter(|w| !is_skip(w)).collect();
    let mut want_rest: Vec<&String> = want.iter().collect();
    got_rest.sort();
    want_rest.sort();
    if got_rest != want_rest {
        failures.push(format!(
            "[{name}] warnings (minus the #58 skips) diverged\n  rust:   {got_rest:?}\n  \
             oracle: {want_rest:?}"
        ));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// P4.147 — dogfood #141 + #142, the property-bag / inform / SQLite-tail riders
// ─────────────────────────────────────────────────────────────────────────────

/// The three entity pointers a restore can preserve: `(table, column)`.
const POINTER_COLUMNS: &[(&str, &str)] = &[
    ("characters", "characterDocumentMountPointId"),
    ("projects", "officialMountPointId"),
    ("groups", "officialMountPointId"),
];

/// The archive's store facts — read off the archive itself, never off either
/// side's dump.
struct ArchiveStores {
    point_ids: HashSet<String>,
    point_names: Vec<String>,
    /// `(table, column, entity id, the archived pointer)`.
    pointers: Vec<(&'static str, &'static str, String, Option<String>)>,
    links_per_point: BTreeMap<String, usize>,
    link_ids: HashSet<String>,
    settings: BTreeMap<String, String>,
}

fn archive_stores(zip: &Path, temp_root: &Path) -> ArchiveStores {
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its stores");
    let d = &extracted.data;
    let id = |v: &Value| {
        v.get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    let mut pointers = Vec::new();
    for (table, column) in POINTER_COLUMNS {
        let rows = match *table {
            "characters" => &d.characters,
            "projects" => &d.projects,
            _ => &d.groups,
        };
        for row in rows {
            pointers.push((
                *table,
                *column,
                id(row),
                row.get(*column).and_then(Value::as_str).map(str::to_string),
            ));
        }
    }
    let mut links_per_point = BTreeMap::new();
    for l in &d.doc_mount_file_links {
        if let Some(mp) = l.get("mountPointId").and_then(Value::as_str) {
            *links_per_point.entry(mp.to_string()).or_insert(0) += 1;
        }
    }
    ArchiveStores {
        point_ids: d.doc_mount_points.iter().map(id).collect(),
        point_names: d
            .doc_mount_points
            .iter()
            .filter_map(|p| p.get("name").and_then(Value::as_str).map(str::to_string))
            .collect(),
        pointers,
        links_per_point,
        link_ids: d.doc_mount_file_links.iter().map(id).collect(),
        settings: d
            .instance_settings
            .iter()
            .filter_map(|r| {
                Some((
                    r.get("key")?.as_str()?.to_string(),
                    r.get("value")?.as_str()?.to_string(),
                ))
            })
            .collect(),
    }
}

fn str_at<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

fn rows_of<'a>(dump: &'a Value, partition: &str, table: &str) -> &'a [Value] {
    dump.get(partition)
        .and_then(|p| p.get(table))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Keep only the rows `keep` accepts; returns how many went.
fn retain_rows(
    dump: &mut Value,
    partition: &str,
    table: &str,
    keep: impl Fn(&Value) -> bool,
) -> usize {
    let Some(rows) = dump
        .get_mut(partition)
        .and_then(|p| p.get_mut(table))
        .and_then(Value::as_array_mut)
    else {
        return 0;
    };
    let before = rows.len();
    rows.retain(|r| keep(r));
    before - rows.len()
}

/// What [`carve_fresh_stores`] did to one side's dump.
#[derive(Default)]
struct CarveOutcome {
    removed: usize,
    rehomed: usize,
    /// Content rows a carved link referenced that SURVIVE because another link
    /// (outside the carved stores) shares them — see [`carve_fresh_stores`].
    shared_content: usize,
    /// Re-homed links given v5's uniquified path (`FRESH_STORE_FOLD_COLLISIONS`).
    renamed: usize,
    /// Post-22d colliding paths no `FRESH_STORE_FOLD_COLLISIONS` entry names.
    unlisted_collisions: Vec<String>,
}

/// ## The legacy fold, landing on the ARCHIVE's vault — part of #141's ruling
///
/// 22f-bis folds a legacy archive's outfit presets into wardrobe items through
/// the character's CURRENT vault. On v4 that is the fresh one, where the item's
/// path is free. On v5 it is the archive's own vault — and
/// `restore-archive-legacy.zip`'s vault ALREADY carries `Wardrobe/Travelling
/// Coat.md` (the fixture is a modern archive plus `outfit-presets.json`), so
/// the wardrobe create uniquifies the fold to `Travelling Coat-1.md`, exactly
/// as v4's own create would on a vault that held it. Same document, another
/// path. Each entry is `(case, the path v4 wrote, the path v5 wrote)`: v4's
/// re-homed link takes v5's path so its content stays under diff, and both
/// directions are asserted (the entry must be USED on v4's side, and v5 must
/// carry the renamed link).
const FRESH_STORE_FOLD_COLLISIONS: &[(&str, &str, &str)] = &[(
    "restore_legacy_archive",
    "Wardrobe/Travelling Coat.md",
    "Wardrobe/Travelling Coat-1.md",
)];

/// Carve v4's #141 fresh stores (`fresh → archive`, keyed by the fresh store's
/// id) out of its dump, re-homing what v4 wrote into a fresh store that the
/// archive's own store does not carry.
///
/// A fresh store holds two kinds of rows. Its PROVISIONING (the vault scaffold
/// and the managed-field projection at phase 6, a project/group store's four
/// files at 13/13a) sits at relative paths the archive's own store also
/// carries — those rows are the divergence, and go. What v4 wrote into it LATER
/// through the entity's pointer sits at paths the archive's store does NOT
/// carry: 22f-bis's legacy wardrobe items (`restore_legacy_archive`), and the
/// compact archive's >3 MB phantom re-ingest (`REPLAY_DEDUPE`). v5 writes those
/// into the archive's store, so they are RE-HOMED — `mountPointId` (and a
/// folder id, by path) rewritten to the archive's store — and stay under diff.
///
/// The store row, its `project_/group_doc_mount_links` row, its carved links'
/// chunks, and the content rows (`doc_mount_files`, their documents and blobs)
/// no surviving link references all go. A content row a carved link shared
/// with a SURVIVING link stays (`doc_mount_files` is global and sha-keyed: a
/// later store's identical file reuses v4's earlier row) — it keeps its v4
/// rowid, earlier than v5's own copy, which is why those two tables then
/// compare order-insensitively on that case ([`FRESH_STORE_SHARED_CONTENT`]).
fn carve_fresh_stores(
    dump: &mut Value,
    fresh_to_archive: &BTreeMap<String, String>,
    archive_link_ids: &HashSet<String>,
    collisions: &[(&str, &str)],
) -> CarveOutcome {
    let mut out = CarveOutcome::default();
    if fresh_to_archive.is_empty() {
        return out;
    }
    let fresh = |r: &Value| str_at(r, "mountPointId").and_then(|m| fresh_to_archive.get(m));
    let key = |p: &str| p.to_lowercase();

    // Folders first: a fresh folder whose path the archive's store carries maps
    // onto that folder; any other is re-homed with its id.
    let folders = rows_of(dump, "mountIndex", "doc_mount_folders").to_vec();
    let mut archive_folder: BTreeMap<(String, String), String> = BTreeMap::new();
    for f in &folders {
        if let (Some(mp), Some(path), Some(id)) = (
            str_at(f, "mountPointId"),
            str_at(f, "path"),
            str_at(f, "id"),
        ) {
            if fresh_to_archive.values().any(|a| a == mp) {
                archive_folder.insert((mp.to_string(), key(path)), id.to_string());
            }
        }
    }
    let mut folder_map: BTreeMap<String, String> = BTreeMap::new();
    let mut rehome_folders: HashSet<String> = HashSet::new();
    for f in &folders {
        let (Some(archive), Some(id)) = (fresh(f), str_at(f, "id")) else {
            continue;
        };
        match archive_folder.get(&(archive.clone(), key(str_at(f, "path").unwrap_or("")))) {
            Some(twin) => {
                folder_map.insert(id.to_string(), twin.clone());
            }
            None => {
                rehome_folders.insert(id.to_string());
            }
        }
    }

    // Links: a path the archive's store carries, written BEFORE 22d restored the
    // archive's links (rowid order), is provisioning — carved. Any other is
    // re-homed; one written AFTER 22d at a path the archive's store also
    // carries collides there on v5, which uniquifies it — the named
    // `FRESH_STORE_FOLD_COLLISIONS` rewrite.
    let links = rows_of(dump, "mountIndex", "doc_mount_file_links").to_vec();
    let first_archive_link = links
        .iter()
        .position(|l| str_at(l, "id").is_some_and(|i| archive_link_ids.contains(i)));
    let mut renames: BTreeMap<String, String> = BTreeMap::new();
    let mut archive_paths: HashSet<(String, String)> = HashSet::new();
    for l in &links {
        if let (Some(mp), Some(path)) = (str_at(l, "mountPointId"), str_at(l, "relativePath")) {
            if fresh_to_archive.values().any(|a| a == mp) {
                archive_paths.insert((mp.to_string(), key(path)));
            }
        }
    }
    let mut gone_links: HashSet<String> = HashSet::new();
    let mut rehome_links: BTreeMap<String, String> = BTreeMap::new();
    let mut gone_link_files: HashSet<String> = HashSet::new();
    for (i, l) in links.iter().enumerate() {
        let (Some(archive), Some(id)) = (fresh(l), str_at(l, "id")) else {
            continue;
        };
        let raw_path = str_at(l, "relativePath").unwrap_or("");
        let collides = archive_paths.contains(&(archive.clone(), key(raw_path)));
        let after_22d = first_archive_link.is_some_and(|f| i > f);
        if !collides {
            rehome_links.insert(id.to_string(), archive.clone());
        } else if after_22d {
            match collisions.iter().find(|(from, _)| *from == raw_path) {
                Some((_, to)) => {
                    rehome_links.insert(id.to_string(), archive.clone());
                    renames.insert(id.to_string(), (*to).to_string());
                }
                None => out.unlisted_collisions.push(raw_path.to_string()),
            }
        } else {
            gone_links.insert(id.to_string());
            if let Some(f) = str_at(l, "fileId") {
                gone_link_files.insert(f.to_string());
            }
        }
    }

    let ids: HashSet<String> = fresh_to_archive.keys().cloned().collect();
    let in_ids = |r: &Value, k: &str| str_at(r, k).is_some_and(|v| ids.contains(v));
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_points", |r| !in_ids(r, "id"));
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_folders", |r| {
        !in_ids(r, "mountPointId") || str_at(r, "id").is_some_and(|i| rehome_folders.contains(i))
    });
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_file_links", |r| {
        !str_at(r, "id").is_some_and(|i| gone_links.contains(i))
    });
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_chunks", |r| {
        !(str_at(r, "linkId").is_some_and(|l| gone_links.contains(l))
            || (in_ids(r, "mountPointId")
                && !str_at(r, "linkId").is_some_and(|l| rehome_links.contains_key(l))))
    });
    out.removed += retain_rows(dump, "mountIndex", "project_doc_mount_links", |r| {
        !in_ids(r, "mountPointId")
    });
    out.removed += retain_rows(dump, "mountIndex", "group_doc_mount_links", |r| {
        !in_ids(r, "mountPointId")
    });

    // Re-home what survives in a fresh store.
    let remap_folder = |v: &Value| -> Value {
        match v.as_str().and_then(|f| folder_map.get(f)) {
            Some(twin) => json!(twin),
            None => v.clone(),
        }
    };
    for table in [
        "doc_mount_folders",
        "doc_mount_file_links",
        "doc_mount_chunks",
    ] {
        let Some(rows) = dump["mountIndex"][table].as_array_mut() else {
            continue;
        };
        for r in rows.iter_mut() {
            let Some(archive) = str_at(r, "mountPointId").and_then(|m| fresh_to_archive.get(m))
            else {
                continue;
            };
            r["mountPointId"] = json!(archive);
            out.rehomed += 1;
            if let Some(to) = str_at(r, "id").and_then(|i| renames.get(i)).cloned() {
                let file_name = to.rsplit('/').next().unwrap_or(&to).to_string();
                r["relativePath"] = json!(to);
                r["fileName"] = json!(file_name);
                out.renamed += 1;
            }
            for k in ["folderId", "parentId"] {
                if let Some(v) = r.get(k).cloned() {
                    r[k] = remap_folder(&v);
                }
            }
        }
    }

    let still_used: HashSet<String> = rows_of(dump, "mountIndex", "doc_mount_file_links")
        .iter()
        .filter_map(|l| str_at(l, "fileId").map(str::to_string))
        .collect();
    out.shared_content = gone_link_files.intersection(&still_used).count();
    let gone_files: HashSet<String> = gone_link_files.difference(&still_used).cloned().collect();
    let in_gone = |r: &Value, k: &str| str_at(r, k).is_some_and(|v| gone_files.contains(v));
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_files", |r| !in_gone(r, "id"));
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_documents", |r| {
        !in_gone(r, "fileId")
    });
    out.removed += retain_rows(dump, "mountIndex", "doc_mount_blobs", |r| {
        !in_gone(r, "fileId")
    });
    out
}

/// One entity's pointer on one side's dump.
fn pointer_on(dump: &Value, table: &str, column: &str, id: &str) -> Option<Option<String>> {
    rows_of(dump, "main", table)
        .iter()
        .find(|r| str_at(r, "id") == Some(id))
        .map(|r| str_at(r, column).map(str::to_string))
}

fn point_exists(dump: &Value, id: &str) -> bool {
    rows_of(dump, "mountIndex", "doc_mount_points")
        .iter()
        .any(|r| str_at(r, "id") == Some(id))
}

/// ## ⚠ THE RULED #141 DIVERGENCE — `FRESH_STORE_RESIDUAL` (P4.147, 2026-10-05)
///
/// v4 restores every character, project and group through its CREATE path,
/// which drops the archived `characterDocumentMountPointId` /
/// `officialMountPointId` and provisions a FRESH vault / official store
/// (`characters.repository.ts:262-296`, `restore.ts:315-345`); 22a then
/// restores the archive's own store under its archived id beside it, orphaned.
/// So every `replace` restore of an archive that carries a pointed store lands
/// TWO stores for the entity and points it at the empty one (dogfood #141:
/// 144 stores from a 77-store archive, Friday's pointer on a 12-file vault
/// while her real 805-link one sat orphaned).
///
/// Under the standing backup/restore ruling (`backup-restore-fix-dont-match`,
/// 2026-08-03) v5 FIXES it on the READ side (Shape A, preserve-at-create): when
/// the archive carries the pointed store, the entity's slim row keeps the
/// pointer and no fresh store is minted. `replace` mode only (ruling R-B).
///
/// Asserted BOTH ways on the RAW dumps, per entity whose archived pointer names
/// an archived store:
///
/// - **v5:** the pointer IS the archive's. (So nothing is carved from v5.)
/// - **v4:** the pointer is NOT the archive's and names a store v4 minted; that
///   store and every mount-index row keyed by it are carved from v4's dump and
///   v4's pointer cell is set to the archive's, so the remainder diffs row for
///   row. **If v4 ever preserves, the v4 direction fires — retire the carve.**
///
/// The FALLBACK arm stays v4-convergent and is pinned as such: an entity whose
/// archived pointer is absent or names a store the archive does not carry
/// (`restore_minimal`, `restore_legacy_profiles_replace`, Riya in
/// `restore_orphan_links_replace`, the project and group in
/// `restore_bag_nulls_replace`) is carved on NEITHER side, and both sides must
/// have minted it a store. Returns `Some(shared content rows kept)` when v4's
/// dump was carved.
fn carve_fresh_store_residual(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    got: &mut BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &mut Value,
    failures: &mut Vec<String>,
) -> Option<usize> {
    let a = archive_stores(zip, temp_root);
    let got_v = serde_json::to_value(&*got).expect("dump serializes");
    let mut carve: BTreeMap<String, String> = BTreeMap::new();
    let mut preserved = 0usize;
    // [P4.158 R-B] First claim wins: `a.pointers` is in phase order
    // (characters 6, projects 13, groups 13a — `POINTER_COLUMNS`) then the
    // archive's row order, so the first entity to name a store keeps it and a
    // later one is expected on the fallback arm, on BOTH sides (v4 mints for
    // everyone). Claims are counted only for entities that restored on both
    // sides, as v5 counts only the ones that reached the preserve check.
    let mut claimed: HashSet<String> = HashSet::new();
    for (table, column, id, archived) in &a.pointers {
        let (g, w) = (
            pointer_on(&got_v, table, column, id),
            pointer_on(want, table, column, id),
        );
        // An entity that did not restore on a side (a refused row) has no
        // pointer to judge; the row diff reports its absence.
        let (Some(g), Some(w)) = (g, w) else {
            continue;
        };
        let first_claim = archived
            .as_ref()
            .filter(|p| a.point_ids.contains(*p))
            .filter(|p| claimed.insert((*p).clone()));
        match first_claim {
            Some(archived) => {
                preserved += 1;
                if g.as_deref() != Some(archived.as_str()) {
                    failures.push(format!(
                        "[{name}] FRESH_STORE_RESIDUAL (v5): {table} {id} points at {g:?}, but \
                         the archive restored its store {archived} — the #141 preserve arm did \
                         not hold"
                    ));
                }
                match w {
                    Some(w) if w == *archived => failures.push(format!(
                        "[{name}] FRESH_STORE_RESIDUAL (v4): {table} {id} now keeps the \
                         archive's store {archived} — v4 has fixed #141; retire the carve"
                    )),
                    Some(w) if !point_exists(want, &w) => failures.push(format!(
                        "[{name}] FRESH_STORE_RESIDUAL (v4): {table} {id} points at {w}, which \
                         names no store on v4's side — not the ruled shape"
                    )),
                    Some(w) => {
                        if carve.insert(w.clone(), archived.clone()).is_some() {
                            failures.push(format!(
                                "[{name}] FRESH_STORE_RESIDUAL (v4): two entities share the \
                                 fresh store {w}"
                            ));
                        }
                    }
                    None => failures.push(format!(
                        "[{name}] FRESH_STORE_RESIDUAL (v4): {table} {id} has NO pointer — not \
                         the ruled shape"
                    )),
                }
                // Point v4's cell at the archive's store so the remainder diffs.
                if let Some(row) = want["main"][*table]
                    .as_array_mut()
                    .and_then(|rows| rows.iter_mut().find(|r| str_at(r, "id") == Some(id)))
                {
                    row[*column] = json!(archived);
                }
            }
            None => {
                // The fallback arm: both sides minted a store, neither carved —
                // an archived pointer the archive does not carry, or (R-B) one
                // an earlier entity already claimed.
                for (side, p, dump) in [("v5", &g, &got_v), ("v4", &w, &*want)] {
                    let ok = p
                        .as_ref()
                        .is_some_and(|p| !a.point_ids.contains(p) && point_exists(dump, p));
                    if !ok {
                        failures.push(format!(
                            "[{name}] FRESH_STORE_RESIDUAL fallback arm ({side}): {table} {id} \
                             (archived pointer {archived:?}, not in the archive or already \
                             claimed) points at {p:?} — both sides must mint it a fresh store"
                        ));
                    }
                }
            }
        }
    }
    if preserved > 0 && carve.len() != preserved {
        failures.push(format!(
            "[{name}] FRESH_STORE_RESIDUAL (v4): {preserved} preservable entities but {} fresh \
             store(s) to carve — every one must get its own",
            carve.len()
        ));
    }
    let collisions: Vec<(&str, &str)> = FRESH_STORE_FOLD_COLLISIONS
        .iter()
        .filter(|(c, _, _)| *c == name)
        .map(|(_, from, to)| (*from, *to))
        .collect();
    let outcome = carve_fresh_stores(want, &carve, &a.link_ids, &collisions);
    if !outcome.unlisted_collisions.is_empty() {
        failures.push(format!(
            "[{name}] FRESH_STORE_RESIDUAL (v4): fresh-store link(s) written after 22d at a path \
             the archive's store also carries, with no FRESH_STORE_FOLD_COLLISIONS entry: {:?}",
            outcome.unlisted_collisions
        ));
    }
    if outcome.renamed != collisions.len() {
        failures.push(format!(
            "[{name}] FRESH_STORE_FOLD_COLLISIONS: {} entr(ies) for this case, {} used on v4's \
             side — a stale entry",
            collisions.len(),
            outcome.renamed
        ));
    }
    let mut got_v = got_v;
    for (from, to) in &collisions {
        let on_v5 = rows_of(&got_v, "mountIndex", "doc_mount_file_links")
            .iter()
            .any(|l| str_at(l, "relativePath") == Some(to));
        if !on_v5 {
            failures.push(format!(
                "[{name}] FRESH_STORE_FOLD_COLLISIONS (v5): no link at {to:?} — the fold no \
                 longer collides on the archive's vault; retire the entry"
            ));
        }
        // The fold re-projects the vault's WHOLE wardrobe (v4's own
        // `createAtLocation`), so on v5 the archive's item already at `from` is
        // rewritten too: its link's `lastModified` takes the write clock and
        // chunk-on-write adds one chunk for it ahead of 22g's archived chunk.
        // Both asserted, then carved, so the rest of the link and every other
        // chunk stay under diff.
        let archive_link = rows_of(&got_v, "mountIndex", "doc_mount_file_links")
            .iter()
            .find(|l| {
                str_at(l, "relativePath") == Some(from)
                    && str_at(l, "id").is_some_and(|i| a.link_ids.contains(i))
            })
            .and_then(|l| str_at(l, "id").map(str::to_string));
        let v4_modified = archive_link.as_ref().and_then(|id| {
            rows_of(want, "mountIndex", "doc_mount_file_links")
                .iter()
                .find(|l| str_at(l, "id") == Some(id))
                .map(|l| l["lastModified"].clone())
        });
        let (Some(link), Some(v4_modified)) = (archive_link, v4_modified) else {
            failures.push(format!(
                "[{name}] FRESH_STORE_FOLD_COLLISIONS: no archived link at {from:?} on both sides"
            ));
            continue;
        };
        let chunk_ids: HashSet<String> = rows_of(&got_v, "mountIndex", "doc_mount_chunks")
            .iter()
            .filter(|c| str_at(c, "linkId") == Some(link.as_str()))
            .filter_map(|c| str_at(c, "id").map(str::to_string))
            .collect();
        let archived_chunks: HashSet<String> = rows_of(want, "mountIndex", "doc_mount_chunks")
            .iter()
            .filter(|c| str_at(c, "linkId") == Some(link.as_str()))
            .filter_map(|c| str_at(c, "id").map(str::to_string))
            .collect();
        let extra: HashSet<String> = chunk_ids.difference(&archived_chunks).cloned().collect();
        if extra.len() != 1 {
            failures.push(format!(
                "[{name}] FRESH_STORE_FOLD_COLLISIONS (v5): {} re-projection chunk(s) on the \
                 archived {from:?}, expected exactly 1",
                extra.len()
            ));
        }
        retain_rows(&mut got_v, "mountIndex", "doc_mount_chunks", |c| {
            !str_at(c, "id").is_some_and(|i| extra.contains(i))
        });
        if let Some(row) = got_v["mountIndex"]["doc_mount_file_links"]
            .as_array_mut()
            .and_then(|rows| {
                rows.iter_mut()
                    .find(|l| str_at(l, "id") == Some(link.as_str()))
            })
        {
            if row["lastModified"] == v4_modified {
                failures.push(format!(
                    "[{name}] FRESH_STORE_FOLD_COLLISIONS (v5): the archived {from:?} kept its \
                     lastModified — the fold no longer re-projects it; narrow the entry"
                ));
            }
            row["lastModified"] = v4_modified;
        }
    }
    if !collisions.is_empty() {
        *got = serde_json::from_value(got_v).expect("dump round-trips");
    }
    if !carve.is_empty() && outcome.removed < carve.len() {
        failures.push(format!(
            "[{name}] FRESH_STORE_RESIDUAL (v4): carved only {} row(s) for {} store(s)",
            outcome.removed,
            carve.len()
        ));
    }
    if !carve.is_empty() {
        println!(
            "  carve {name}: {} fresh store(s), {} row(s) carved, {} re-homed, {} shared \
             content row(s) kept",
            carve.len(),
            outcome.removed,
            outcome.rehomed,
            outcome.shared_content
        );
    }
    (!carve.is_empty()).then_some(outcome.shared_content)
}

/// The `replace` cases whose v4 dump carries a #141 fresh store to carve —
/// every `replace` case whose archive carries a store some entity points at
/// (measured, P4.147). A case falling out of the count is the carve going
/// vacuous.
/// P4.161: + its four refusal archives (all `restore-archive.zip` derivations).
const FRESH_STORE_CARVED_CASES: usize = 28;

/// The `summary` adjustments a ruled carve makes: v5 leads v4's `files`
/// counter by `files_lead`, and these v4 warnings are the divergence itself.
#[derive(Default)]
struct SummaryCarve {
    files_lead: i64,
    drop_v4_warnings: Vec<String>,
    /// [`FRESH_STORE_SHARED_CONTENT`] applies on this case.
    shared_content: bool,
    /// [`PRESERVE_BACKFILL`] applies on this case.
    preserve_backfill: bool,
}

/// The two content tables a [`carve_fresh_stores`] that kept a SHARED content
/// row compares order-insensitively (canonical labelling, every value still
/// compared): v4's fresh store created that row first, so it keeps an earlier
/// rowid than v5's own later copy — the same row, another insertion slot.
/// Part of `FRESH_STORE_RESIDUAL`, not a divergence of its own.
const FRESH_STORE_SHARED_CONTENT: &[&str] = &["doc_mount_files", "doc_mount_documents"];

const UPLOADS_UNPROVISIONED: &str = "Quilltap Uploads mount has not been provisioned";

/// ## ⚠ THE RULED #142 DIVERGENCE — `FRESH_TARGET_UPLOADS` (P4.147, 2026-10-05)
///
/// The files phase resolves Quilltap Uploads through the TARGET's
/// `instance_settings.userUploadsMountPointId`, which the `replace` wipe leaves
/// alone and 22o only overwrites LAST. On a FRESH target (or one whose Uploads
/// store was re-minted) that pointer names the target's own, now-wiped store,
/// so every project-less file the restore must replay fails `Quilltap Uploads
/// mount has not been provisioned` — on v4 too (`restore.ts:508-512,569-576`).
/// v5 (ruling R-C) pre-applies the archive's built-in pointers right after 22a
/// when they name a store 22a restored, so those files land in the ARCHIVE's
/// Uploads store.
///
/// Each entry is `(case, the project-less files only the fix restores)`.
/// `restore_gen2_fresh_replace` carries NONE (both its files are carried by the
/// archive's own store rows, so neither side ever asks the bridge): it is the
/// fresh-target convergence control. The three ALIGNED cases are the other
/// controls — the fix is a no-op there, and both sides must carry zero Uploads
/// warnings.
///
/// Both ways on the raw dumps: **v4** warns exactly once per file and writes no
/// `files` row for it (if v4 restores one, it has fixed #142 — retire the
/// entry); **v5** warns NOWHERE, writes each row on `mount-blob:<the archive's
/// Uploads id>:…`, and ends with `userUploadsMountPointId` naming that existing
/// store. Then v5's rows for those files (the `files` row, its blob, its link,
/// the content row nothing else references) are carved and the summary moves by
/// exactly the files' count.
const FRESH_TARGET_UPLOADS: &[(&str, &[&str])] = &[
    ("restore_uploads_fresh_replace", &["portrait.png"]),
    ("restore_compact_fresh_replace", &["portrait.png"]),
    ("restore_gen2_fresh_replace", &[]),
];

fn uploads_warnings(warnings: &Value) -> Vec<String> {
    warnings
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|w| w.contains(UPLOADS_UNPROVISIONED))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn carve_fresh_target_uploads(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    summary: &RestoreSummary,
    want_summary: &Value,
    got: &mut BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    carve: &mut SummaryCarve,
    failures: &mut Vec<String>,
) {
    let got_warnings = json!(summary.warnings);
    if aligns_uploads_pointer(name) {
        // The aligned controls: the pre-apply rewrites the value the target
        // already holds, so neither side may warn.
        for (side, w) in [("v5", &got_warnings), ("v4", &want_summary["warnings"])] {
            let hits = uploads_warnings(w);
            if !hits.is_empty() {
                failures.push(format!(
                    "[{name}] FRESH_TARGET_UPLOADS aligned control ({side}): {hits:?}"
                ));
            }
        }
        return;
    }
    let Some((_, files)) = FRESH_TARGET_UPLOADS.iter().find(|(c, _)| *c == name) else {
        return;
    };
    let a = archive_stores(zip, temp_root);
    let uploads = a
        .settings
        .get("userUploadsMountPointId")
        .cloned()
        .unwrap_or_else(|| panic!("{name}: the archive carries no Uploads pointer"));
    let mut got_v = serde_json::to_value(&*got).expect("dump serializes");

    // v4: one warning per file, no row.
    let v4_hits = uploads_warnings(&want_summary["warnings"]);
    let expected: Vec<String> = files
        .iter()
        .map(|f| format!("Failed to restore file \"{f}\": {UPLOADS_UNPROVISIONED}"))
        .collect();
    if v4_hits != expected {
        failures.push(format!(
            "[{name}] FRESH_TARGET_UPLOADS (v4): Uploads warnings {v4_hits:?}, expected \
             {expected:?} — if v4 restores them now it has fixed #142; retire the entry"
        ));
    }
    let v4_files = rows_of(want, "main", "files");
    for f in *files {
        if v4_files
            .iter()
            .any(|r| str_at(r, "originalFilename") == Some(f))
        {
            failures.push(format!(
                "[{name}] FRESH_TARGET_UPLOADS (v4): v4 restored {f:?} — #142 fixed upstream; \
                 retire the entry"
            ));
        }
    }

    // v5: no warning, every row on the archive's Uploads store, the pointer
    // resolving.
    let v5_hits = uploads_warnings(&got_warnings);
    if !v5_hits.is_empty() {
        failures.push(format!(
            "[{name}] FRESH_TARGET_UPLOADS (v5): {v5_hits:?} — the archive's Uploads pointer \
             was not pre-applied before the files phase"
        ));
    }
    let v5_pointer = rows_of(&got_v, "main", "instance_settings")
        .iter()
        .find(|r| str_at(r, "key") == Some("userUploadsMountPointId"))
        .and_then(|r| str_at(r, "value").map(str::to_string));
    if v5_pointer.as_deref() != Some(uploads.as_str()) || !point_exists(&got_v, &uploads) {
        failures.push(format!(
            "[{name}] FRESH_TARGET_UPLOADS (v5): userUploadsMountPointId {v5_pointer:?} must be \
             the archive's {uploads} AND name an existing store"
        ));
    }
    let prefix = format!("mount-blob:{uploads}:");
    let mut blob_ids: HashSet<String> = HashSet::new();
    for f in *files {
        let row = rows_of(&got_v, "main", "files")
            .iter()
            .find(|r| str_at(r, "originalFilename") == Some(f));
        match row.and_then(|r| str_at(r, "storageKey")) {
            Some(key) if key.starts_with(&prefix) => {
                blob_ids.insert(key[prefix.len()..].to_string());
            }
            other => failures.push(format!(
                "[{name}] FRESH_TARGET_UPLOADS (v5): {f:?} restored with storageKey {other:?}, \
                 expected one on the archive's Uploads store ({prefix}…)"
            )),
        }
    }

    // Carve v5's replayed rows: the `files` rows, their blobs, the content rows
    // the blobs hang off, every link to those content rows in the Uploads store,
    // and the links' chunks.
    let content_ids: HashSet<String> = rows_of(&got_v, "mountIndex", "doc_mount_blobs")
        .iter()
        .filter(|b| str_at(b, "id").is_some_and(|i| blob_ids.contains(i)))
        .filter_map(|b| str_at(b, "fileId").map(str::to_string))
        .collect();
    let link_ids: HashSet<String> = rows_of(&got_v, "mountIndex", "doc_mount_file_links")
        .iter()
        .filter(|l| {
            str_at(l, "mountPointId") == Some(uploads.as_str())
                && str_at(l, "fileId").is_some_and(|f| content_ids.contains(f))
        })
        .filter_map(|l| str_at(l, "id").map(str::to_string))
        .collect();
    // [P4.158 R-F] the folders those links sat in (the replay's `restored/`)
    // go with them when nothing else uses them and the archive does not carry
    // them — on a fresh target v4's replay never runs, so it never makes one.
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its folders");
    let archive_folders: HashSet<String> = extracted
        .data
        .doc_mount_folders
        .iter()
        .filter_map(|f| str_at(f, "id").map(str::to_string))
        .collect();
    let link_folders: HashSet<String> = rows_of(&got_v, "mountIndex", "doc_mount_file_links")
        .iter()
        .filter(|l| str_at(l, "id").is_some_and(|i| link_ids.contains(i)))
        .filter_map(|l| str_at(l, "folderId").map(str::to_string))
        .collect();
    retain_rows(&mut got_v, "main", "files", |r| {
        !str_at(r, "originalFilename").is_some_and(|f| files.contains(&f))
    });
    retain_rows(&mut got_v, "mountIndex", "doc_mount_file_links", |r| {
        !str_at(r, "id").is_some_and(|i| link_ids.contains(i))
    });
    let used_folders: HashSet<String> = rows_of(&got_v, "mountIndex", "doc_mount_file_links")
        .iter()
        .filter_map(|l| str_at(l, "folderId").map(str::to_string))
        .collect();
    retain_rows(&mut got_v, "mountIndex", "doc_mount_folders", |r| {
        !str_at(r, "id").is_some_and(|i| {
            link_folders.contains(i) && !used_folders.contains(i) && !archive_folders.contains(i)
        })
    });
    retain_rows(&mut got_v, "mountIndex", "doc_mount_chunks", |r| {
        !str_at(r, "linkId").is_some_and(|i| link_ids.contains(i))
    });
    let still_used: HashSet<String> = rows_of(&got_v, "mountIndex", "doc_mount_file_links")
        .iter()
        .filter_map(|l| str_at(l, "fileId").map(str::to_string))
        .collect();
    let gone: HashSet<String> = content_ids.difference(&still_used).cloned().collect();
    retain_rows(&mut got_v, "mountIndex", "doc_mount_blobs", |r| {
        !str_at(r, "id").is_some_and(|i| blob_ids.contains(i))
    });
    retain_rows(&mut got_v, "mountIndex", "doc_mount_files", |r| {
        !str_at(r, "id").is_some_and(|i| gone.contains(i))
    });
    retain_rows(&mut got_v, "mountIndex", "doc_mount_documents", |r| {
        !str_at(r, "fileId").is_some_and(|i| gone.contains(i))
    });
    *got = serde_json::from_value(got_v).expect("dump round-trips");
    carve.files_lead = files.len() as i64;
    carve.drop_v4_warnings.extend(v4_hits);
}

/// ## [P4.147 item 10(b)] The message replay's serde arm — a RECORDED divergence
///
/// `restore-archive-informs.zip`'s chat c…0002 ("Quiet Interlude") carries one
/// message whose `content` is the NUMBER 5. v4's `addMessage` refuses it with
/// the ZodError of `ChatEventSchema` — a plain `z.union`, so the message is ONE
/// `invalid_union` issue whose `errors` hold every member's nested issues
/// (measured: the message member's `["content"]` type miss first). v5's typed
/// decode refuses it with serde's sentence. Both SKIP it with `Failed to
/// restore message in chat "Quiet Interlude": …`; only the tail differs.
///
/// **Why it stays a divergence:** `api/zod_issues.rs`'s
/// `zod_chat_event_issues` answers the collapsed union with `errors: []` —
/// sufficient for v4's WARN projection, NOT v4's `ZodError.message` (P4.143
/// Tier 3 item 13; the schema-shape generator is P4.143 item 12). So v5 cannot
/// produce these bytes without that table, and this pins the difference by
/// name instead: VANISHED if the tails agree, WRONG SHAPE unless v4's tail is a
/// Zod message whose first issue is an `invalid_union` with the `["content"]`
/// type miss first, and v5's starts with [`MESSAGE_SERDE_V5_PREFIX`].
const MESSAGE_SERDE_CASE: &str = "restore_informs_replace";
const MESSAGE_SERDE_HEAD: &str = "Failed to restore message in chat \"Quiet Interlude\": ";
const MESSAGE_SERDE_V5_PREFIX: &str = "invalid type: integer `5`, expected a string";

fn classify_message_serde_arm(
    name: &str,
    got: &Value,
    want: &Value,
    failures: &mut Vec<String>,
) -> (Value, Value) {
    let tail_of = |warnings: &Value| -> Option<String> {
        warnings
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find_map(|w| w.strip_prefix(MESSAGE_SERDE_HEAD).map(str::to_string))
    };
    let is_v4_shape = |tail: &str| {
        is_zod_error_message(tail)
            && serde_json::from_str::<Value>(tail).ok().is_some_and(|v| {
                v[0]["code"] == "invalid_union"
                    && v[0]["path"] == json!([])
                    && v[0]["errors"][0][0]["path"] == json!(["content"])
            })
    };
    let (g, w) = (tail_of(got), tail_of(want));
    match (&g, &w) {
        (Some(g), Some(w)) if g == w => failures.push(format!(
            "[{name}] the message serde-arm divergence VANISHED — both sides say {g:?}; retire \
             the pin"
        )),
        (Some(g), Some(w)) if is_v4_shape(w) && g.starts_with(MESSAGE_SERDE_V5_PREFIX) => {}
        _ => failures.push(format!(
            "[{name}] the message serde-arm divergence has the WRONG SHAPE\n  rust:   {g:?}\n  \
             oracle: {w:?}"
        )),
    }
    let carve = |warnings: &Value| -> Value {
        let mut out = warnings.clone();
        if let Some(ws) = out.as_array_mut() {
            for w in ws.iter_mut() {
                if w.as_str()
                    .is_some_and(|s| s.starts_with(MESSAGE_SERDE_HEAD))
                {
                    *w = json!(format!("{MESSAGE_SERDE_HEAD}<SERDE-ARM-DIVERGENCE>"));
                }
            }
        }
        out
    };
    (carve(got), carve(want))
}

/// ## ⚠ THE RULED R-A DIVERGENCE — `PRESERVE_BACKFILL` (P4.158, 2026-10-06)
///
/// A preserved store (P4.147's #141 preserve arm) can be INCOMPLETE — an
/// archive whose vault lacks a managed file (`restore-archive-damaged-store
/// .zip` drops `description.md` from Lorian's vault and from the project and
/// group stores). v4 cannot reach the state: it never preserves, it projects
/// every managed field from the archived row into a FRESH store, so each
/// description comes back from the row. v5 keeps the archive's store and,
/// since R-A, BACKFILLS each missing managed file from the same row after the
/// mount family — so the restored entity reads what the archive's row said
/// (refusing the preserve instead would lose the store's OTHER files).
///
/// Each entry is `(case, [(table, entity id, store id, path)])`. Both ways on
/// the dumps: **both** sides carry the file on the archive's store holding the
/// archived row's `description` — or, for every other managed file (P4.161),
/// v4's projected bytes (v4's after the #141 carve re-homed its fresh
/// projection — so a v4 that stops projecting fails here too); **v5**'s link is
/// written AFTER the archive's own links (a backfill), **v4**'s BEFORE them (the
/// phase-6/13 projection) — the insertion-order difference this divergence IS,
/// so [`PRESERVE_BACKFILL_TABLES`] then compare order-insensitively (every value
/// still compared).
/// `(table, entity id, store id, path)`.
type BackfillEntry = (&'static str, &'static str, &'static str, &'static str);

const PRESERVE_BACKFILL: &[(&str, &[BackfillEntry])] = &[(
    "restore_damaged_store_replace",
    &[
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "description.md",
        ),
        (
            "projects",
            "a3000000-0000-4000-8000-000000000001",
            "5c17e916-5f79-4cca-a134-ec09c05924e9",
            "description.md",
        ),
        (
            "groups",
            "a2000000-0000-4000-8000-000000000001",
            "60a8194d-f8ea-4540-af77-5e13e7ed0e9b",
            "description.md",
        ),
        // [P4.161 Tier 2 item 10] the backfill's OTHER managed files, one of
        // each kind (the re-derived archive strips them too).
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "properties.json",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "metadata.json",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "identity.md",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "manifesto.md",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "personality.md",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "example-dialogues.md",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "physical-description.md",
        ),
        (
            "characters",
            "a1000000-0000-4000-8000-000000000001",
            "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",
            "physical-prompts.json",
        ),
        (
            "projects",
            "a3000000-0000-4000-8000-000000000001",
            "5c17e916-5f79-4cca-a134-ec09c05924e9",
            "properties.json",
        ),
        (
            "projects",
            "a3000000-0000-4000-8000-000000000001",
            "5c17e916-5f79-4cca-a134-ec09c05924e9",
            "state.json",
        ),
        (
            "groups",
            "a2000000-0000-4000-8000-000000000001",
            "60a8194d-f8ea-4540-af77-5e13e7ed0e9b",
            "instructions.md",
        ),
    ],
)];

/// ## ⚠ THE RULED R-A DIVERGENCE, ITS CONTENT ROWS — `PRESERVE_BACKFILL_SHARED_CONTENT` (P4.161)
///
/// A backfilled managed file whose BYTES the archive already holds (the empty
/// `""` markdown, the `{}` JSON, a `properties.json` the projection renders
/// identically) shares the archive's content row on v5: the backfill runs
/// AFTER 22b restored the archive's `doc_mount_files` / `doc_mount_documents`,
/// and the write path dedupes by sha256. v4 projects the same file in phase
/// 6 / 13, BEFORE 22b, so its projection mints its own content row and 22b
/// then adds the archive's — two rows with one sha256, each link reading the
/// same bytes. Measured on the re-derived `restore-archive-damaged-store.zip`
/// (P4.161 Tier 2 item 10 — the original `description.md` damage never hit
/// it: every description's bytes are unique in the archive).
///
/// Each entry is `(case, [sha256])`. Both ways: **v4** carries EXACTLY two
/// `doc_mount_files` rows per listed sha (one of them the archive's), **v5**
/// exactly one (the archive's) — a v4 that stops duplicating, or a v5 that
/// starts, fails here. Then v4's minted duplicate is folded onto the archive's
/// row (its `doc_mount_documents` row dropped, its links repointed) so the
/// content tables diff row for row.
const PRESERVE_BACKFILL_SHARED_CONTENT: &[(&str, &[&str])] = &[(
    "restore_damaged_store_replace",
    &[
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a",
        "70eb827a041e489d4478af17c56a212827a3933ad946999d49bb026aacab67bc",
    ],
)];

fn collapse_backfill_shared_content(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &mut Value,
    failures: &mut Vec<String>,
) {
    let Some((_, shas)) = PRESERVE_BACKFILL_SHARED_CONTENT
        .iter()
        .find(|(c, _)| *c == name)
    else {
        return;
    };
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its content rows");
    let archived: HashSet<String> = extracted
        .data
        .doc_mount_files
        .iter()
        .filter_map(|f| str_at(f, "id").map(str::to_string))
        .collect();
    let got_v = serde_json::to_value(got).expect("dump serializes");
    let mut fold: BTreeMap<String, String> = BTreeMap::new();
    for sha in *shas {
        let ids = |dump: &Value| -> Vec<String> {
            rows_of(dump, "mountIndex", "doc_mount_files")
                .iter()
                .filter(|f| str_at(f, "sha256") == Some(sha))
                .filter_map(|f| str_at(f, "id").map(str::to_string))
                .collect()
        };
        let (g, w) = (ids(&got_v), ids(want));
        let g_ok = g.len() == 1 && archived.contains(&g[0]);
        let w_archive: Vec<&String> = w.iter().filter(|i| archived.contains(*i)).collect();
        if !g_ok || w.len() != 2 || w_archive.len() != 1 {
            failures.push(format!(
                "[{name}] PRESERVE_BACKFILL_SHARED_CONTENT {sha}: v5 rows {g:?} (want the \
                 archive's one), v4 rows {w:?} (want the archive's + one minted) — if v4 \
                 converged, retire the entry"
            ));
            continue;
        }
        let keep = w_archive[0].clone();
        for id in w.iter().filter(|i| **i != keep) {
            fold.insert(id.clone(), keep.clone());
        }
    }
    if fold.is_empty() {
        return;
    }
    for table in ["doc_mount_files", "doc_mount_documents"] {
        let key = if table == "doc_mount_files" {
            "id"
        } else {
            "fileId"
        };
        if let Some(rows) = want["mountIndex"][table].as_array_mut() {
            rows.retain(|r| !str_at(r, key).is_some_and(|id| fold.contains_key(id)));
        }
    }
    if let Some(rows) = want["mountIndex"]["doc_mount_file_links"].as_array_mut() {
        for r in rows.iter_mut() {
            if let Some(to) = str_at(r, "fileId").and_then(|f| fold.get(f)).cloned() {
                r["fileId"] = Value::String(to);
            }
        }
    }
}

/// The four tables a backfill writes into, compared order-insensitively on a
/// [`PRESERVE_BACKFILL`] case.
const PRESERVE_BACKFILL_TABLES: &[&str] = &[
    "doc_mount_file_links",
    "doc_mount_chunks",
    "doc_mount_files",
    "doc_mount_documents",
];

/// The `(rowid position, content)` of the link at `(store, path)` on one side.
fn store_file(dump: &Value, store: &str, path: &str) -> Option<(usize, Option<String>)> {
    let links = rows_of(dump, "mountIndex", "doc_mount_file_links");
    let (i, link) = links.iter().enumerate().find(|(_, l)| {
        str_at(l, "mountPointId") == Some(store) && str_at(l, "relativePath") == Some(path)
    })?;
    let content = str_at(link, "fileId").and_then(|f| {
        rows_of(dump, "mountIndex", "doc_mount_documents")
            .iter()
            .find(|d| str_at(d, "fileId") == Some(f))
            .and_then(|d| str_at(d, "content").map(str::to_string))
    });
    Some((i, content))
}

fn assert_preserve_backfill(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) -> bool {
    let Some((_, entries)) = PRESERVE_BACKFILL.iter().find(|(c, _)| *c == name) else {
        return false;
    };
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its rows");
    let d = &extracted.data;
    let a = archive_stores(zip, temp_root);
    let got_v = serde_json::to_value(got).expect("dump serializes");
    for (table, id, store, path) in *entries {
        let rows = match *table {
            "characters" => &d.characters,
            "projects" => &d.projects,
            _ => &d.groups,
        };
        // `description.md` is pinned to the archived row's value (P4.158);
        // every other managed file (P4.161 Tier 2 item 10) to v4's projected
        // bytes on the same store and path — the backfill must render what
        // the projection renders.
        let archived = if *path == "description.md" {
            rows.iter()
                .find(|r| str_at(r, "id") == Some(id))
                .and_then(|r| str_at(r, "description"))
                .map(str::to_string)
        } else {
            store_file(want, store, path).and_then(|(_, c)| c)
        };
        // The store's links in the archive, so "before/after 22d" is measured
        // against the archive's own first link on that store.
        let first_archive_link = |dump: &Value| {
            rows_of(dump, "mountIndex", "doc_mount_file_links")
                .iter()
                .position(|l| {
                    str_at(l, "mountPointId") == Some(store)
                        && str_at(l, "id").is_some_and(|i| a.link_ids.contains(i))
                })
        };
        for (side, dump, written_after) in [("v5", &got_v, true), ("v4", want, false)] {
            match (store_file(dump, store, path), first_archive_link(dump)) {
                (Some((i, content)), Some(first)) => {
                    if content != archived {
                        failures.push(format!(
                            "[{name}] PRESERVE_BACKFILL ({side}): {table} {id} {path} on {store} \
                             reads {content:?}, the archived row says {archived:?}"
                        ));
                    }
                    if (i > first) != written_after {
                        failures.push(format!(
                            "[{name}] PRESERVE_BACKFILL ({side}): {table} {id} {path} sits at \
                             rowid slot {i}, the archive's first link at {first} — expected it \
                             {} the archive's links",
                            if written_after { "after" } else { "before" }
                        ));
                    }
                }
                (file, first) => failures.push(format!(
                    "[{name}] PRESERVE_BACKFILL ({side}): {table} {id} has no {path} on {store} \
                     ({file:?}, first archived link {first:?}) — the backfill (v5) / the fresh \
                     projection (v4) did not land it"
                )),
            }
        }
    }
    true
}

/// ## ⚠ THE RULED R-C DIVERGENCE — `GENERAL_POINTER_PREAPPLY` (P4.158, 2026-10-06)
///
/// 22f-bis files a SHARED legacy wardrobe item (`characterId: null`) in
/// Quilltap General, resolved through `instance_settings.generalMountPointId`.
/// v4 restores the pointers LAST (22o), so on a target whose General is not
/// the archive's — any fresh instance: a minted id, wiped by the `replace`
/// delete — it writes the archetype (and a `Wardrobe` folder) into a store
/// that no longer exists, while the instance comes out pointing at the
/// archive's General: the item is unreachable. Measured on v4's own dump of
/// `restore_general_pointer_fresh_replace` (P4.158 R-C: the condition the
/// ruling named). v5 shared it until 22a-ter moved to right after 22a.
///
/// Each entry is `(case, the archetype's relative path)`. Both ways on the RAW
/// dumps: **v5** — the link sits on the store the restored
/// `generalMountPointId` names, and that store exists; **v4** — the link sits
/// on a store that is NOT the restored pointer and does not exist (if v4 ever
/// lands it on its restored General, it has converged — retire the entry).
/// Then every v4 mount-index row keyed by that dangling store is re-homed onto
/// v4's restored General, so the remainder diffs row for row.
const GENERAL_POINTER_PREAPPLY: &[(&str, &str)] = &[(
    "restore_general_pointer_fresh_replace",
    "Wardrobe/The Shared Greatcoat.md",
)];

fn general_pointer(dump: &Value) -> Option<String> {
    rows_of(dump, "main", "instance_settings")
        .iter()
        .find(|r| str_at(r, "key") == Some("generalMountPointId"))
        .and_then(|r| str_at(r, "value").map(str::to_string))
}

fn link_store(dump: &Value, path: &str) -> Option<String> {
    rows_of(dump, "mountIndex", "doc_mount_file_links")
        .iter()
        .find(|l| str_at(l, "relativePath") == Some(path))
        .and_then(|l| str_at(l, "mountPointId").map(str::to_string))
}

fn carve_general_pointer_preapply(
    name: &str,
    got: &mut BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &mut Value,
    failures: &mut Vec<String>,
) {
    let Some((_, path)) = GENERAL_POINTER_PREAPPLY.iter().find(|(c, _)| *c == name) else {
        return;
    };
    let got_v = serde_json::to_value(&*got).expect("dump serializes");
    let (g_general, g_store) = (general_pointer(&got_v), link_store(&got_v, path));
    if g_general.is_none()
        || g_store != g_general
        || !g_store.as_deref().is_some_and(|s| point_exists(&got_v, s))
    {
        failures.push(format!(
            "[{name}] GENERAL_POINTER_PREAPPLY (v5): {path:?} on store {g_store:?}, the restored \
             generalMountPointId is {g_general:?} — the archetype must land in the General the \
             instance comes out pointing at"
        ));
    }
    let (w_general, w_store) = (general_pointer(want), link_store(want, path));
    let (Some(w_general), Some(dangling)) = (w_general, w_store) else {
        failures.push(format!(
            "[{name}] GENERAL_POINTER_PREAPPLY (v4): no restored General pointer or no {path:?} link"
        ));
        return;
    };
    if dangling == w_general || point_exists(want, &dangling) {
        failures.push(format!(
            "[{name}] GENERAL_POINTER_PREAPPLY (v4): {path:?} is on {dangling}, which v4's \
             restored pointer names or which exists — v4 has converged; retire the entry"
        ));
        return;
    }
    // The archetype's chunk: v4's chunk-on-write gates on the store EXISTING
    // (`reindex-file.ts:71-72` — `docMountPoints.findById`, then
    // `mountType === 'database'`), so its dangling write gets none and its link
    // reads `chunkCount: 0`; v5's lands in a live store and is chunked. Asserted
    // both ways, then v5's chunk rows are carved and its count set to v4's.
    let link_id = |dump: &Value| {
        rows_of(dump, "mountIndex", "doc_mount_file_links")
            .iter()
            .find(|l| str_at(l, "relativePath") == Some(path))
            .and_then(|l| str_at(l, "id").map(str::to_string))
    };
    let chunks_of = |dump: &Value, link: &Option<String>| {
        rows_of(dump, "mountIndex", "doc_mount_chunks")
            .iter()
            .filter(|c| link.is_some() && str_at(c, "linkId") == link.as_deref())
            .count()
    };
    let (g_link, w_link) = (link_id(&got_v), link_id(want));
    let (g_chunks, w_chunks) = (chunks_of(&got_v, &g_link), chunks_of(want, &w_link));
    if g_chunks == 0 || w_chunks != 0 {
        failures.push(format!(
            "[{name}] GENERAL_POINTER_PREAPPLY: the archetype's chunks — v5 {g_chunks} (expected \
             >0, a live store), v4 {w_chunks} (expected 0, a missing store)"
        ));
    }
    let mut got_v = got_v;
    retain_rows(&mut got_v, "mountIndex", "doc_mount_chunks", |c| {
        g_link.is_none() || str_at(c, "linkId") != g_link.as_deref()
    });
    if let Some(row) = got_v["mountIndex"]["doc_mount_file_links"]
        .as_array_mut()
        .and_then(|rows| {
            rows.iter_mut()
                .find(|l| str_at(l, "relativePath") == Some(path))
        })
    {
        row["chunkCount"] = json!(0);
    }
    *got = serde_json::from_value(got_v).expect("dump round-trips");

    let mut moved = 0usize;
    for table in [
        "doc_mount_folders",
        "doc_mount_file_links",
        "doc_mount_chunks",
    ] {
        let Some(rows) = want["mountIndex"][table].as_array_mut() else {
            continue;
        };
        for r in rows.iter_mut() {
            if str_at(r, "mountPointId") == Some(dangling.as_str()) {
                r["mountPointId"] = json!(w_general);
                moved += 1;
            }
        }
    }
    println!("  general-pointer {name}: {moved} v4 row(s) re-homed from {dangling}");
}

/// ## [P4.158 R-B] The v5-only claim WARN — pinned, with its silence leg
///
/// v4 cannot reach a second claimant: its create drops every archived pointer
/// and mints a fresh store (`characters.repository.ts:253`,
/// `store-backed.repository.ts:137`), so it logs nothing a v5 line could match
/// — this is a RECORDED v5-only line. Each entry is `(case, [the line's fields
/// after the message, in order])`; every case NOT listed must log none.
const CLAIMED_STORE_WARN: &str =
    "WARN quilltap::restore Archived store already claimed by an earlier entity; falling back to a fresh store";
const CLAIMED_STORE_WARNS: &[(&str, &[&str])] = &[(
    "restore_two_claimants_replace",
    &[
        "entity=character entityId=a1000000-0000-4000-8000-000000000002 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc \
         claimedBy=a1000000-0000-4000-8000-000000000001",
        "entity=group entityId=a2000000-0000-4000-8000-000000000001 \
         mountPointId=5c17e916-5f79-4cca-a134-ec09c05924e9 \
         claimedBy=a3000000-0000-4000-8000-000000000001",
    ],
)];

/// ## [P4.158 R-A] The v5-only backfill WARN — pinned, with its silence leg
///
/// One line per managed file [`PRESERVE_BACKFILL`]'s completeness pass wrote.
/// v4 never preserves, so it never backfills and has no line: RECORDED v5-only.
const BACKFILL_WARN: &str =
    "WARN quilltap::restore Backfilled a managed file the archived store was missing";
const BACKFILL_WARNS: &[(&str, &[&str])] = &[(
    "restore_damaged_store_replace",
    // [P4.161 Tier 2 item 10] + the eleven other managed files the re-derived
    // archive strips, in the backfill's own file order.
    &[
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=properties.json",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=metadata.json",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=identity.md",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=description.md",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=manifesto.md",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=personality.md",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=example-dialogues.md",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=physical-description.md",
        "entity=character entityId=a1000000-0000-4000-8000-000000000001 \
         mountPointId=aff3114e-ed90-4d5b-99c1-ef3fa20203fc relativePath=physical-prompts.json",
        "entity=project entityId=a3000000-0000-4000-8000-000000000001 \
         mountPointId=5c17e916-5f79-4cca-a134-ec09c05924e9 relativePath=properties.json",
        "entity=project entityId=a3000000-0000-4000-8000-000000000001 \
         mountPointId=5c17e916-5f79-4cca-a134-ec09c05924e9 relativePath=description.md",
        "entity=project entityId=a3000000-0000-4000-8000-000000000001 \
         mountPointId=5c17e916-5f79-4cca-a134-ec09c05924e9 relativePath=state.json",
        "entity=group entityId=a2000000-0000-4000-8000-000000000001 \
         mountPointId=60a8194d-f8ea-4540-af77-5e13e7ed0e9b relativePath=description.md",
        "entity=group entityId=a2000000-0000-4000-8000-000000000001 \
         mountPointId=60a8194d-f8ea-4540-af77-5e13e7ed0e9b relativePath=instructions.md",
    ],
)];

/// Every v5-only restore line, each `(its prefix, the cases that log it with
/// their field tails)`; every case not listed must log none of it.
/// `(case, the field tails it logs)`.
type V5OnlyCases = &'static [(&'static str, &'static [&'static str])];

const V5_ONLY_RESTORE_LINES: &[(&str, V5OnlyCases)] = &[
    (CLAIMED_STORE_WARN, CLAIMED_STORE_WARNS),
    (BACKFILL_WARN, BACKFILL_WARNS),
];

fn assert_claimed_store_warns(name: &str, lines: &[String], failures: &mut Vec<String>) {
    for (prefix, cases) in V5_ONLY_RESTORE_LINES {
        let got: Vec<&str> = lines
            .iter()
            .filter_map(|l| l.strip_prefix(*prefix))
            .map(str::trim_start)
            .collect();
        let want: Vec<&str> = cases
            .iter()
            .find(|(c, _)| *c == name)
            .map(|(_, w)| w.to_vec())
            .unwrap_or_default();
        if got != want {
            failures.push(format!(
                "[{name}] the v5-only line {prefix:?}: got {got:?}, expected {want:?}"
            ));
        }
    }
}

/// ## [P4.158 R-G] The restore's log census — every `moduleLogger` line of
/// ## v4's `restore.ts`, on every case
///
/// All 63 sites (44 warn, 5 info, 14 debug — each message distinct), recorded
/// by the oracle on every case with every context key. v5 must log the same
/// lines, in the same order, with the same fields (an Error recorded as its
/// message; an object / array through the `…Json` file-layer convention).
/// The P4.158 lane record holds the census table: which arms v5 had, which it
/// lacked (ported), and which are unreachable in v5 (named, with the reason).
const RESTORE_TS_MESSAGES: &[&str] = &[
    "All entities restored with preserved IDs - no reconciliation needed",
    "Failed to enqueue reindex after compact restore",
    "Failed to restore LLM log",
    "Failed to restore character",
    "Failed to restore character plugin data",
    "Failed to restore chat",
    "Failed to restore chat document",
    "Failed to restore chat inform",
    "Failed to restore chat settings",
    "Failed to restore connection profile",
    "Failed to restore conversation annotation",
    "Failed to restore conversation chunk",
    "Failed to restore doc mount blob",
    "Failed to restore doc mount chunk",
    "Failed to restore doc mount document",
    "Failed to restore doc mount file",
    "Failed to restore doc mount file link",
    "Failed to restore doc mount folder",
    "Failed to restore doc mount point",
    "Failed to restore embedding profile",
    "Failed to restore embedding status",
    "Failed to restore file",
    "Failed to restore folder",
    "Failed to restore group",
    "Failed to restore group character member",
    "Failed to restore group doc mount link",
    "Failed to restore image profile",
    "Failed to restore instance setting",
    "Failed to restore memory",
    "Failed to restore npm plugin",
    "Failed to restore plugin config",
    "Failed to restore project",
    "Failed to restore project doc mount link",
    "Failed to restore prompt template",
    "Failed to restore provider model",
    "Failed to restore roleplay template",
    "Failed to restore tag",
    "Failed to restore text replacement rule",
    "Failed to restore tfidf vocabulary",
    "Failed to restore theme bundle",
    "Failed to restore themes-index.json",
    "Failed to restore vector entries batch",
    "Failed to restore vector index meta",
    "Failed to restore wardrobe item",
    "No npm plugins directory in backup",
    "No themes directory in backup",
    "Post-restore embedding reconcile complete",
    "Queued full re-index for compact backup restore",
    "Renamed connection profile on restore to avoid name collision",
    "Restore operation completed",
    "Restored chat informs",
    "Restored npm plugin",
    "Restored npm plugins",
    "Restored text replacement rules",
    "Restored theme bundle",
    "Restored user-installed theme bundles",
    "Seeded connection-profile columns the archive predates",
    "Skipped duplicate folder row during restore",
    "Skipping LLM logs restore — logs database is in degraded mode",
    "Skipping duplicate text replacement rule on restore",
    "Starting restore operation",
    "Translated pre-4.10 Concierge settings for restore",
    "Translated the retired impersonated-line voice toggle for restore",
];

/// The repository-level lines (validation, `_create`'s rethrow, the chats
/// wrap) beneath a refused restore create — compared on the two cases P4.147
/// wired them on (through P4.149's `db::fallback` homes).
const REPO_LEVEL_MESSAGES: &[&str] = &[
    "Data validation failed",
    "Error creating entity",
    "Failed to create chat",
    // [P4.161] the memories wrap and the store-backed pair.
    "Error creating memory",
    "Error creating project entity",
    "Error creating group entity",
    "Error creating project",
    "Error creating group",
    // [P4.161 Tier 2] the per-kind repository wraps.
    "Error creating prompt template",
    "Error creating folder",
    "Error creating tag",
];
const REPO_LEVEL_CASES: &[&str] = &[
    "restore_informs_replace",
    "restore_sqlite_tail_replace",
    // [P4.161] the three whole-row refusal archives.
    "restore_memory_refusals_replace",
    "restore_inform_refusals_replace",
    "restore_entity_refusals_replace",
    "restore_kind_refusals_replace",
];

/// v5's captured lines (`LEVEL target message k=v …`) whose message is in
/// `messages`, as `{level, message, <fields>}` records — in the line's own
/// field ORDER (P4.161 Tier 2 item 9: [`compare_restore_logs`] compares key
/// order). A field boundary is a space followed by `name=`; `error` runs to
/// the end of the line (v5 logs it LAST — a ZodError can hold anything); a
/// `…Json` field's value is parsed as ONE JSON value, so a field may follow
/// it (it used to run to the end of the line, which forced `summaryJson` last
/// where v4 logs `summary` before `warningCount`), and it is stored under its
/// v4 name with its JSON re-rendered compactly, as the oracle renders an
/// object.
fn v5_log_records(lines: &[String], messages: &[&str]) -> Vec<Value> {
    let mut out = Vec::new();
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let (Some(level), Some(_target), Some(rest)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(message) = messages
            .iter()
            .filter(|m| rest == **m || rest.starts_with(&format!("{m} ")))
            .max_by_key(|m| m.len())
        else {
            continue;
        };
        let mut rec = Map::new();
        rec.insert("level".into(), json!(level.to_lowercase()));
        rec.insert("message".into(), json!(message));
        let mut fields = rest[message.len()..].trim_start();
        while !fields.is_empty() {
            let Some((key, value_and_rest)) = fields.split_once('=') else {
                break;
            };
            let to_end = key == "error";
            let json_len = key.ends_with("Json").then(|| {
                let mut it =
                    serde_json::Deserializer::from_str(value_and_rest).into_iter::<Value>();
                match it.next() {
                    Some(Ok(_)) => it.byte_offset(),
                    _ => value_and_rest.len(),
                }
            });
            let (value, next) = if to_end {
                (value_and_rest, "")
            } else if let Some(n) = json_len {
                (&value_and_rest[..n], value_and_rest[n..].trim_start())
            } else {
                match find_field_boundary(value_and_rest) {
                    Some(i) => (&value_and_rest[..i], value_and_rest[i + 1..].trim_start()),
                    None => (value_and_rest, ""),
                }
            };
            match key.strip_suffix("Json") {
                Some(name) => {
                    let v = serde_json::from_str::<Value>(value)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| value.to_string());
                    rec.insert(name.to_string(), json!(v));
                }
                None => {
                    rec.insert(key.to_string(), json!(value));
                }
            }
            fields = next;
        }
        out.push(Value::Object(rec));
    }
    out
}

/// The index of the space that starts the next ` name=` field, if any.
fn find_field_boundary(s: &str) -> Option<usize> {
    s.char_indices()
        .filter(|(_, c)| *c == ' ')
        .map(|(i, _)| i)
        .find(|&i| {
            let tail = &s[i + 1..];
            tail.split_once('=').is_some_and(|(k, _)| {
                !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        })
}

/// v4's recorded `logs` with every field rendered as v5's capture renders it
/// (a string bare; a number, boolean, null or object as its compact JSON).
fn v4_log_records(want: &[Value], messages: &[&str]) -> Vec<Value> {
    want.iter()
        .filter(|l| str_at(l, "message").is_some_and(|m| messages.contains(&m)))
        .map(|l| {
            let mut rec = Map::new();
            for (k, v) in l.as_object().into_iter().flatten() {
                let s = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                rec.insert(k.clone(), json!(s));
            }
            Value::Object(rec)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn compare_restore_logs(
    name: &str,
    want: &Value,
    got_lines: &[String],
    got_summary: &RestoreSummary,
    want_summary: &Value,
    carve: &SummaryCarve,
    literals: &HashSet<String>,
    failures: &mut Vec<String>,
) {
    let Some(want) = want.as_array() else {
        failures.push(format!(
            "[{name}] the oracle carries no `logs` — regenerate it"
        ));
        return;
    };
    let mut messages: Vec<&str> = RESTORE_TS_MESSAGES.to_vec();
    // [P4.161 Tier 2 item 9] v4's `restore.ts` lines all log through ONE
    // module logger, so v5's twins all live on ONE target —
    // `quilltap::restore` (P4.158 left five on the default module target).
    for l in got_lines {
        let mut parts = l.splitn(3, ' ');
        let (Some(_), Some(target), Some(rest)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        if target != "quilltap::restore"
            && RESTORE_TS_MESSAGES
                .iter()
                .any(|m| rest == *m || rest.starts_with(&format!("{m} ")))
        {
            failures.push(format!(
                "[{name}] a restore.ts line on target {target:?}, not \"quilltap::restore\": {l}"
            ));
        }
    }
    if REPO_LEVEL_CASES.contains(&name) {
        messages.extend_from_slice(REPO_LEVEL_MESSAGES);
    }
    let (mut g, mut w) = (
        v5_log_records(got_lines, &messages),
        v4_log_records(want, &messages),
    );
    // `Restore operation completed` carries the whole summary and its warning
    // count: each side must log ITS OWN returned summary (the summaries
    // themselves are compared — carves and all — by `compare_case`), then both
    // fields leave the line diff.
    let got_summary_v = serde_json::to_value(got_summary).expect("summary serializes");
    for (side, recs, summary) in [("v5", &mut g, &got_summary_v), ("v4", &mut w, want_summary)] {
        for r in recs.iter_mut() {
            if r["message"] != "Restore operation completed" {
                continue;
            }
            let logged = r
                .get("summary")
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            if logged.as_ref() != Some(summary) {
                failures.push(format!(
                    "[{name}] `Restore operation completed` ({side}) logs a summary that is not \
                     the one the restore returned"
                ));
            }
            let n = summary["warnings"].as_array().map(Vec::len).unwrap_or(0);
            if r.get("warningCount").and_then(Value::as_str) != Some(n.to_string().as_str()) {
                failures.push(format!(
                    "[{name}] `Restore operation completed` ({side}) warningCount {:?}, its \
                     summary carries {n}",
                    r.get("warningCount")
                ));
            }
            // Their VALUES leave the line diff (each side checked against its
            // own summary above); their POSITIONS stay (P4.161 item 9 — v4
            // logs `{targetUserId, mode, summary, warningCount}`).
            if let Some(o) = r.as_object_mut() {
                for k in ["summary", "warningCount"] {
                    if let Some(v) = o.get_mut(k) {
                        *v = json!("<own summary>");
                    }
                }
            }
        }
    }
    // The ruled divergences whose v4 warnings are carved from the summary
    // carry a WARN line each: the same lines leave v4's side.
    let dedupe =
        REPLAY_DEDUPE.contains(&name) && !REPLAY_DEDUPE_NARROWED.iter().any(|(c, _)| *c == name);
    w.retain(|r| {
        let m = r["message"].as_str().unwrap_or("");
        let e = r["error"].as_str().unwrap_or("");
        !(dedupe
            && (m == "Failed to restore doc mount folder"
                || m == "Failed to restore doc mount file link"))
            && !(carve.files_lead != 0
                && m == "Failed to restore file"
                && e == UPLOADS_UNPROVISIONED)
    });
    // [P4.143 item 2] the serde arm's chat: v4's ZodError vs v5's serde
    // sentence (pinned by shape in `compare_repo_logs`) — the tail leaves.
    if name == SERDE_ROOM_CASE {
        for r in g.iter_mut().chain(w.iter_mut()) {
            if r["message"] == "Failed to restore chat"
                && r["chatId"] == "c1000000-0000-4000-8000-000000000006"
            {
                r["error"] = json!("<SERDE-ARM-DIVERGENCE>");
            }
        }
    }
    // The ruled files-phase slot (see `files_phase_last`).
    let (mut g, mut w) = (
        files_phase_last(g, |r: &Value| r["message"] == "Failed to restore file"),
        files_phase_last(w, |r: &Value| r["message"] == "Failed to restore file"),
    );
    // The family's origin rule: a UUID the archive does not carry was minted
    // (the `new-account` remap), so it is labelled rather than compared.
    for r in g.iter_mut().chain(w.iter_mut()) {
        if let Some(o) = r.as_object_mut() {
            for v in o.values_mut() {
                if v.as_str()
                    .is_some_and(|s| is_uuid(s) && !literals.contains(s))
                {
                    *v = json!("<minted>");
                }
            }
        }
    }
    // [P4.161 Tier 2 item 9] KEY ORDER is compared: `serde_json::Map`
    // equality (an `IndexMap` under `preserve_order`) ignores it, so each
    // record becomes its ordered `[key, value]` pairs.
    let ordered = |recs: &[Value]| -> Vec<Value> {
        recs.iter()
            .map(|r| {
                Value::Array(
                    r.as_object()
                        .into_iter()
                        .flatten()
                        .map(|(k, v)| json!([k, v]))
                        .collect(),
                )
            })
            .collect()
    };
    let (g, w) = (ordered(&g), ordered(&w));
    if g != w {
        let detail = g
            .iter()
            .zip(w.iter())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| {
                format!("first difference at line {i}:\n    rust:   {a}\n    oracle: {b}")
            })
            .unwrap_or_else(|| format!("line count: rust {} vs oracle {}", g.len(), w.len()));
        failures.push(format!(
            "[{name}] the restore's log census differs — {detail}\n  rust:   {:?}\n  oracle: {:?}",
            g.iter()
                .map(|r| r[1][1].as_str().unwrap_or(""))
                .collect::<Vec<_>>(),
            w.iter()
                .map(|r| r[1][1].as_str().unwrap_or(""))
                .collect::<Vec<_>>()
        ));
    }
}

/// [P4.147 item 8] **The fallback arm's whole property bag**, asserted
/// within-tree on BOTH sides (so a two-sided loss cannot pass as agreement).
/// `restore-archive-bag-nulls.zip` drops the project and group stores, so each
/// takes the fresh-store arm; the fresh store's `properties.json` must carry
/// every bag key the archive row carries, explicit `null`s included — v5's old
/// `project_properties` copied SIX of the project's sixteen, and the group's
/// value-or-absent create dropped `color: null`.
fn assert_bag_nulls_survive(
    name: &str,
    zip: &Path,
    temp_root: &Path,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    if name != "restore_bag_nulls_replace" {
        return;
    }
    use quilltap_core::db::document_store_overlay::StoreEntity;
    let extracted = quilltap_core::services::backup::restore::parse_backup_zip(zip, temp_root)
        .expect("parse archive for its bags");
    let got_v = serde_json::to_value(got).expect("dump serializes");
    use quilltap_core::db::groups::GroupEntity;
    use quilltap_core::db::projects::ProjectEntity;
    // The bag each side must hold: the archived row's bag keys through the
    // entity's own parse — which keeps every explicit `null` and applies the
    // read-side normalizations v4's Zod applies too (a pre-4.9
    // `backgroundDisplayMode: "project"` reads as `"theme"` on both sides).
    let parsed = |table: &str, raw: &Value, keys: &[&str]| -> Value {
        let bag: Map<String, Value> = keys
            .iter()
            .filter_map(|k| raw.get(*k).map(|v| ((*k).to_string(), v.clone())))
            .collect();
        let bag = Value::Object(bag);
        match table {
            "projects" => serde_json::to_value(ProjectEntity::parse_properties(&bag).unwrap()),
            _ => serde_json::to_value(GroupEntity::parse_properties(&bag).unwrap()),
        }
        .unwrap()
    };
    let entities: [(&str, &Value, &[&str]); 2] = [
        (
            "projects",
            &extracted.data.projects[0],
            ProjectEntity::property_keys(),
        ),
        (
            "groups",
            &extracted.data.groups[0],
            GroupEntity::property_keys(),
        ),
    ];
    for (table, archived, keys) in entities {
        let expected = parsed(table, archived, keys);
        let id = str_at(archived, "id").unwrap();
        let carried = keys.iter().filter(|k| archived.get(**k).is_some()).count();
        if carried != keys.len() {
            failures.push(format!(
                "[{name}] BAG-NULLS fixture: the archived {table} row carries {carried} of {} \
                 bag keys — rebuild the derived archive",
                keys.len()
            ));
        }
        for (side, dump) in [("v5", &got_v), ("v4", want)] {
            let bag = pointer_on(dump, table, "officialMountPointId", id)
                .flatten()
                .and_then(|mp| {
                    let link = rows_of(dump, "mountIndex", "doc_mount_file_links")
                        .iter()
                        .find(|l| {
                            str_at(l, "mountPointId") == Some(mp.as_str())
                                && str_at(l, "relativePath") == Some("properties.json")
                        })?;
                    let file = str_at(link, "fileId")?;
                    let doc = rows_of(dump, "mountIndex", "doc_mount_documents")
                        .iter()
                        .find(|d| str_at(d, "fileId") == Some(file))?;
                    serde_json::from_str::<Value>(str_at(doc, "content")?).ok()
                });
            let Some(bag) = bag else {
                failures.push(format!(
                    "[{name}] BAG-NULLS ({side}): {table} {id} has no readable properties.json"
                ));
                continue;
            };
            for k in keys {
                if bag.get(*k).is_none() || bag.get(*k) != expected.get(*k) {
                    failures.push(format!(
                        "[{name}] BAG-NULLS ({side}): {table}.{k} = {:?}, the archive's bag \
                         parses to {:?}",
                        bag.get(*k),
                        expected.get(*k)
                    ));
                }
            }
        }
    }
}

/// [P4.147 item 9] **The archived informs, by name** — v4's `ChatInformSchema`
/// keeps `true` / `false` / absent (→ `false`) and the consumed row, and
/// REFUSES `null` / `"true"` / `1`; v5 converged on that (ruling R-D). Both
/// sides must land exactly rows 01/02/03/07 with `permanent` 1/0/0/0 and the
/// consumed row's message id, and neither may land 04/05/06.
const INFORMS_CASE: &str = "restore_informs_replace";

fn assert_informs_restored(
    name: &str,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    if name != INFORMS_CASE {
        return;
    }
    let got_v = serde_json::to_value(got).expect("dump serializes");
    let expected = json!([
        ["a9000000-0000-4000-8000-000000000001", 1, null],
        ["a9000000-0000-4000-8000-000000000002", 0, null],
        ["a9000000-0000-4000-8000-000000000003", 0, null],
        [
            "a9000000-0000-4000-8000-000000000007",
            0,
            "d1000000-0000-4000-8000-000000000002"
        ],
    ]);
    for (side, dump) in [("v5", &got_v), ("v4", want)] {
        let mut rows: Vec<Value> = rows_of(dump, "main", "chat_informs")
            .iter()
            .map(|r| json!([r["id"], r["permanent"], r["consumedByMessageId"]]))
            .collect();
        rows.sort_by_key(|r| r[0].as_str().unwrap_or("").to_string());
        if Value::Array(rows.clone()) != expected {
            failures.push(format!(
                "[{name}] INFORMS ({side}): restored {rows:?}, expected {expected}"
            ));
        }
    }
}

/// ## ⚠ THE RULED DIVERGENCE — `INDEX_KEYED_EMBEDDING` (the human, 2026-10-07)
///
/// v4's FULL backup writes `data.memories` raw, so every embedding is
/// `JSON.stringify(Float32Array)` — an index-keyed OBJECT `{"0":…}` — which
/// v4's OWN restore refuses in `MemorySchema`'s embedding union (an
/// `invalid_union` warning per memory: every embedded memory is lost). Ruled
/// at the `94fbb1ae3` boot-hardness unification: FIX v5 —
/// `restore::rows::decode_index_keyed_embedding` decodes that exact shape to
/// the union's `number[]`, so the memory and its vector restore. (Before P4.161
/// v5 restored such a memory with a NULL vector; P4.161's whole-row parse had
/// silently converged on v4's loss.)
///
/// Each entry is `(case, memory id, the decoded vector)`. Asserted BOTH ways
/// — v4 refuses that id with its `Failed to restore memory` WARN and three
/// repository ERRORs carrying the same error; v5 lands it with exactly the
/// decoded vector's BLOB and logs none of them — then carved from every
/// comparand, so a v4 convergence (or a v5 regression) trips it.
const INDEX_KEYED_EMBEDDING: &[(&str, &str, &[f32])] = &[(
    "restore_memory_refusals_replace",
    "ad000000-0000-4000-8000-000000000009",
    &[0.25],
)];

fn carve_index_keyed_embedding(
    name: &str,
    got: &mut BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want_state: &mut Value,
    case: &mut Value,
    drop_v4_warnings: &mut Vec<String>,
    failures: &mut Vec<String>,
) {
    for (_, id, vector) in INDEX_KEYED_EMBEDDING.iter().filter(|(c, _, _)| *c == name) {
        // v5: the memory landed, carrying exactly the decoded vector.
        let blob = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(
                quilltap_core::embedding_blob::float32_to_blob(vector)
            ))
        );
        let rows = got
            .get_mut("main")
            .and_then(|p| p.get_mut("memories"))
            .expect("v5 dump has main.memories");
        match rows.iter().position(|r| r["id"] == *id) {
            Some(i) => {
                if rows[i]["embedding"] != json!(blob) {
                    failures.push(format!(
                        "[{name}] INDEX_KEYED_EMBEDDING (v5): {id} landed with embedding {}, \
                         want the decoded {vector:?} ({blob})",
                        rows[i]["embedding"]
                    ));
                }
                rows.remove(i);
            }
            None => failures.push(format!(
                "[{name}] INDEX_KEYED_EMBEDDING (v5): {id} did not restore — the decode is gone"
            )),
        }
        // v4: refused, with ONE `Failed to restore memory` WARN naming the id…
        if rows_of(want_state, "main", "memories")
            .iter()
            .any(|r| r["id"] == *id)
        {
            failures.push(format!(
                "[{name}] INDEX_KEYED_EMBEDDING (v4): {id} now RESTORES on v4 — v4 converged; \
                 retire the divergence"
            ));
            continue;
        }
        let logs = case["logs"].as_array_mut().expect("oracle carries logs");
        let Some(at) = logs
            .iter()
            .position(|l| l["message"] == "Failed to restore memory" && l["memoryId"] == *id)
        else {
            failures.push(format!(
                "[{name}] INDEX_KEYED_EMBEDDING (v4): no `Failed to restore memory` for {id}"
            ));
            continue;
        };
        let error = logs.remove(at)["error"].as_str().unwrap_or("").to_string();
        if !error.contains("invalid_union") {
            failures.push(format!(
                "[{name}] INDEX_KEYED_EMBEDDING (v4): {id} refused for another reason: {error}"
            ));
        }
        // …its three repository ERRORs (validate, `_create`, the wrap) on the
        // same error — recorded in the case's `logs` census…
        let before = logs.len();
        logs.retain(|l| {
            !(l["error"] == json!(error)
                && matches!(
                    l["message"].as_str(),
                    Some(
                        "Data validation failed"
                            | "Error creating entity"
                            | "Error creating memory"
                    )
                ))
        });
        if before - logs.len() != 3 {
            failures.push(format!(
                "[{name}] INDEX_KEYED_EMBEDDING (v4): {} repository line(s) carry {id}'s error, \
                 want 3",
                before - logs.len()
            ));
        }
        // …and the post-restore reconcile's count: v5's landed memory carries
        // a vector of the DECODED width, so where that differs from the
        // target profile's dimensions v5's `mismatched.memories` counts it
        // and v4's (which never saw the row) does not.
        for r in logs.iter_mut().filter(|l| {
            l["message"] == "Post-restore embedding reconcile complete"
                && l["targetDimensions"].as_u64() != Some(vector.len() as u64)
        }) {
            if let Some(n) = r["mismatched"]["memories"].as_u64() {
                r["mismatched"]["memories"] = json!(n + 1);
            }
        }
        // …and its one summary warning, which `compare_case` drops (v4's own
        // summary stays whole: its `Restore operation completed` line is
        // checked against it).
        let warning = format!("Failed to restore memory: {error}");
        if case["summary"]["warnings"]
            .as_array()
            .is_some_and(|ws| ws.iter().any(|w| *w == json!(warning)))
        {
            drop_v4_warnings.push(warning);
        } else {
            failures.push(format!(
                "[{name}] INDEX_KEYED_EMBEDDING (v4): no summary warning for {id}"
            ));
        }
    }
}

/// [P4.161 — dogfood #152, §S.2, P4.155's R-B] **The whole-row refusal
/// archives, by name, on BOTH sides** (so a two-sided loss cannot pass as
/// agreement): each side lands EXACTLY the sound rows its derive script names
/// and none of the refused ones; the memory whose defaulted keys are absent
/// lands with `MemorySchema`'s defaults (R-C — v5 used to write 5.0 / `AUTO`
/// / 0 / 0); and on v5 the archive's own project keeps its archived store —
/// the refused project that named that store claimed nothing (v4 always
/// mints, so this half is v5's alone).
fn assert_refusals_restored(
    name: &str,
    got: &BTreeMap<String, BTreeMap<String, Vec<Value>>>,
    want: &Value,
    failures: &mut Vec<String>,
) {
    let got_v = serde_json::to_value(got).expect("dump serializes");
    let ids_of = |dump: &Value, table: &str| -> Vec<String> {
        let mut v: Vec<String> = rows_of(dump, "main", table)
            .iter()
            .filter_map(|r| r["id"].as_str().map(str::to_string))
            .collect();
        v.sort();
        v
    };
    let ids = |dump: &Value, partition: &str, table: &str, prefix: &str| -> Vec<String> {
        let mut v: Vec<String> = rows_of(dump, partition, table)
            .iter()
            .filter_map(|r| r["id"].as_str())
            .filter(|id| id.starts_with(prefix) || prefix.is_empty())
            .map(str::to_string)
            .collect();
        v.sort();
        v
    };
    match name {
        "restore_memory_refusals_replace" => {
            let expected: Vec<String> = ["01", "02", "11", "12", "13"]
                .iter()
                .map(|n| format!("ad000000-0000-4000-8000-0000000000{n}"))
                .collect();
            for (side, dump) in [("v5", &got_v), ("v4", want)] {
                let landed = ids(dump, "main", "memories", "");
                if landed != expected {
                    failures.push(format!(
                        "[{name}] MEMORIES ({side}): restored {landed:?}, expected {expected:?}"
                    ));
                }
                let row = rows_of(dump, "main", "memories")
                    .iter()
                    .find(|r| r["id"] == "ad000000-0000-4000-8000-000000000012");
                let got_defaults = row.map(|r| {
                    json!([
                        r["importance"],
                        r["source"],
                        r["kind"],
                        r["reinforcementCount"],
                        r["reinforcedImportance"]
                    ])
                });
                if got_defaults != Some(json!([0.5, "MANUAL", "semantic", 1, 0.5])) {
                    failures.push(format!(
                        "[{name}] R-C DEFAULTS ({side}): {got_defaults:?}, expected \
                         [0.5, MANUAL, semantic, 1, 0.5]"
                    ));
                }
            }
        }
        "restore_inform_refusals_replace" => {
            let expected = vec![
                "af000000-0000-4000-8000-000000000010".to_string(),
                "af000000-0000-4000-8000-000000000011".to_string(),
            ];
            for (side, dump) in [("v5", &got_v), ("v4", want)] {
                let landed = ids(dump, "main", "chat_informs", "");
                if landed != expected {
                    failures.push(format!(
                        "[{name}] INFORMS ({side}): restored {landed:?}, expected {expected:?}"
                    ));
                }
            }
        }
        "restore_kind_refusals_replace" => {
            // Prompt templates mint fresh ids on restore, so they are named.
            let expected = vec!["Fixture Prompt", "Sound Template Twin"];
            for (side, dump) in [("v5", &got_v), ("v4", want)] {
                let mut names: Vec<&str> = rows_of(dump, "main", "prompt_templates")
                    .iter()
                    .filter(|r| r["isBuiltIn"] != json!(1) && r["isBuiltIn"] != json!(true))
                    .filter_map(|r| r["name"].as_str())
                    .collect();
                names.sort();
                if names != expected {
                    failures.push(format!(
                        "[{name}] PROMPT TEMPLATES ({side}): restored {names:?}, expected {expected:?}"
                    ));
                }
                let folders = ids_of(dump, "folders");
                let want_folders = vec![
                    "a9000000-0000-4000-8000-000000000001".to_string(),
                    "a9000000-0000-4000-8000-0000000000e4".to_string(),
                ];
                let tags = ids_of(dump, "tags");
                let want_tags: Vec<String> = ["01", "02", "e4", "e5"]
                    .iter()
                    .map(|n| format!("a5000000-0000-4000-8000-0000000000{n}"))
                    .collect();
                if tags != want_tags {
                    failures.push(format!(
                        "[{name}] TAGS ({side}): restored {tags:?}, expected {want_tags:?}"
                    ));
                }
                if folders != want_folders {
                    failures.push(format!(
                        "[{name}] FOLDERS ({side}): restored {folders:?}, expected {want_folders:?}"
                    ));
                }
            }
        }
        "restore_entity_refusals_replace" => {
            for (side, dump) in [("v5", &got_v), ("v4", want)] {
                for (table, expected) in [
                    (
                        "projects",
                        vec![
                            "a3000000-0000-4000-8000-000000000001",
                            "a3000000-0000-4000-8000-0000000000e3",
                        ],
                    ),
                    (
                        "groups",
                        vec![
                            "a2000000-0000-4000-8000-000000000001",
                            "a2000000-0000-4000-8000-0000000000e2",
                        ],
                    ),
                ] {
                    let landed = ids(dump, "main", table, "");
                    if landed != expected {
                        failures.push(format!(
                            "[{name}] {table} ({side}): restored {landed:?}, expected {expected:?}"
                        ));
                    }
                }
            }
            let voyage = rows_of(&got_v, "main", "projects")
                .iter()
                .find(|r| r["id"] == "a3000000-0000-4000-8000-000000000001")
                .map(|r| r["officialMountPointId"].clone());
            if voyage != Some(json!("5c17e916-5f79-4cca-a134-ec09c05924e9")) {
                failures.push(format!(
                    "[{name}] v5: The Voyage must keep its archived store (the refused \
                     project that named it claims nothing): {voyage:?}"
                ));
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_case(
    name: &str,
    summary: &RestoreSummary,
    case: &Value,
    summary_carve: &SummaryCarve,
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
    // [P4.158 R-F] a narrowed dedupe case compares its warnings and summary.
    let narrowed = REPLAY_DEDUPE_NARROWED.iter().any(|(c, _)| *c == name);
    let got_summary = serde_json::to_value(summary).expect("summary serializes");
    let want_summary = &case["summary"];
    for (k, gv) in got_summary.as_object().unwrap() {
        let wv = &want_summary[k];
        // [P4.147] `FRESH_TARGET_UPLOADS`: v5 restores exactly the carved
        // files more than v4 does (asserted both ways in the carve).
        if k == "files" && summary_carve.files_lead != 0 {
            let (g, w) = (gv.as_i64().unwrap_or(-1), wv.as_i64().unwrap_or(-1));
            if g != w + summary_carve.files_lead {
                failures.push(format!(
                    "[{name}] summary.files: rust {g} vs oracle {w} — expected rust to lead by \
                     exactly {} (FRESH_TARGET_UPLOADS)",
                    summary_carve.files_lead
                ));
            }
            continue;
        }
        if k == "warnings" {
            // [P4.147] the v4 warnings that ARE the #142 divergence.
            let mut wv = wv.clone();
            if let Some(ws) = wv.as_array_mut() {
                for drop in &summary_carve.drop_v4_warnings {
                    if let Some(i) = ws.iter().position(|w| w.as_str() == Some(drop.as_str())) {
                        ws.remove(i);
                    }
                }
            }
            let wv = &wv;
            // The dedupe cases' warnings ARE the divergence — v4 reports its own
            // refused rows and v5 reports nothing. Asserted, not diffed. The
            // orphan case's warnings are likewise the #58 divergence: v5 adds
            // exactly the skip sentences, v4 must have none.
            if name == ORPHAN_LINKS_CASE {
                assert_orphan_warnings(name, gv, wv, failures);
            } else if name == SERDE_ROOM_CASE {
                // [P4.143 item 2] the serde arm's ONE warning is a recorded
                // divergence — classified and carved, then the rest verbatim.
                let (g, w) = classify_restore_serde_arm(name, gv, wv, failures);
                compare_warnings(name, &g, &w, failures);
            } else if name == MESSAGE_SERDE_CASE {
                // [P4.147 item 10(b)] the message replay's serde arm, likewise.
                let (g, w) = classify_message_serde_arm(name, gv, wv, failures);
                compare_warnings(name, &g, &w, failures);
            } else if !dedupe || narrowed {
                compare_warnings(name, gv, wv, failures);
            }
            continue;
        }
        if dedupe && !narrowed && REPLAY_DEDUPE_SUMMARY_KEYS.contains(&k.as_str()) {
            continue;
        }
        // The #58 orphan case's three doc-store counters ARE the divergence:
        // they count WRITTEN rows, so v5 (which skips the orphans) must trail
        // v4 by exactly the orphan counts — both directions, like the tables.
        if name == ORPHAN_LINKS_CASE {
            let delta = match k.as_str() {
                "docMountFileLinks" => Some(ORPHAN_COUNTS.0 as i64),
                "docMountFolders" => Some(ORPHAN_COUNTS.1 as i64),
                "docMountChunks" => Some(ORPHAN_COUNTS.2 as i64),
                _ => None,
            };
            if let Some(delta) = delta {
                let g = gv.as_i64().unwrap_or(-1);
                let w = wv.as_i64().unwrap_or(-1);
                if w != g + delta {
                    failures.push(format!(
                        "[{name}] summary.{k}: rust {g} vs oracle {w} — expected the oracle to \
                         lead by exactly {delta} (the #58 orphans v4 inserts silently). If the \
                         two now agree, v4 has adopted the skip check: re-rule the divergence."
                    ));
                }
                continue;
            }
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

    // The #58 orphan case needs each side's parent id sets before the loop —
    // the orphan predicate is per-side (v4 restores the orphaned links, so a
    // chunk that is orphaned on the v5 side has a live parent on v4's).
    let orphan_case = name == ORPHAN_LINKS_CASE;
    let (got_points, want_points, got_links, want_links) = if orphan_case {
        let empty = Vec::new();
        let g_mi = got.get("mountIndex");
        let g_pts = table_ids(
            g_mi.and_then(|t| t.get("doc_mount_points"))
                .unwrap_or(&empty),
        );
        let g_lnk = table_ids(
            g_mi.and_then(|t| t.get("doc_mount_file_links"))
                .unwrap_or(&empty),
        );
        let w_pts = table_ids(
            &want["mountIndex"]["doc_mount_points"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        // The want-side chunk parent set is the HEALTHY links only: v4 restored
        // the point-orphaned links too, so a chunk hanging off one has a live
        // linkId row — its orphanhood is transitive through the missing store.
        let w_rows = want["mountIndex"]["doc_mount_file_links"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let w_lnk: HashSet<String> = w_rows
            .iter()
            .filter(|r| {
                r.get("mountPointId")
                    .and_then(Value::as_str)
                    .is_some_and(|v| w_pts.contains(v))
            })
            .filter_map(|r| r.get("id").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        (g_pts, w_pts, g_lnk, w_lnk)
    } else {
        Default::default()
    };

    for (partition, tables) in got {
        for (table, rows) in tables {
            // The ruled dedupe divergence: these five hold different rows by
            // design. `assert_replay_dedupe` above states exactly how, in both
            // directions, instead of diffing them.
            if dedupe && replay_dedupe_tables(name).contains(&(partition.as_str(), table.as_str()))
            {
                continue;
            }
            // [P4.D46] On the compact cases the SAME ruled divergence reaches
            // one more table: v4's unconditional re-ingest fires a best-effort
            // `refreshStats`, and the new 24a/25 steps' awaits give that
            // fire-and-forget chain time to land before v4's dump — so v4's
            // doc_mount_points rollups reflect rows v5 (which skips the
            // re-ingest by ruling) never wrote. The four older dedupe cases
            // predate the tail and still compare this table green, so the
            // mask is deliberately confined to the compact pair.
            if matches!(
                name,
                "restore_compact_replace"
                    | "restore_compact_new_account"
                    | "restore_compact_fresh_replace"
            ) && partition == "mountIndex"
                && table == "doc_mount_points"
            {
                continue;
            }
            // ONE view of both sides per table so every branch below — the
            // plain diff, the phase-order residual and the #58 orphan assertion
            // — reads the same rows. (P4.D145 found the two helpers reading the
            // raw rows while the plain diff read a masked copy, so a carve-out
            // silently did not apply to them; the P4.D146 carve-out that
            // exposed it was retired at the round's unification.)
            let mut g_rows = rows.clone();
            let mut w_rows = want[partition][table]
                .as_array()
                .cloned()
                .unwrap_or_default();
            // The #58 orphan divergence: three tables asserted in both
            // directions instead of diffed; the healthy remainder still
            // compared row for row inside the helper.
            if orphan_case && partition == "mountIndex" && ORPHAN_TABLES.contains(&table.as_str()) {
                failures.extend(assert_orphan_divergence(
                    name,
                    table,
                    &g_rows,
                    &w_rows,
                    &got_points,
                    &want_points,
                    &got_links,
                    &want_links,
                    &literals,
                    &shas_got,
                    &shas_want,
                ));
                continue;
            }
            let mut n_got = Normalizer::new(literals.clone(), shas_got.clone());
            let mut n_want = Normalizer::new(literals.clone(), shas_want.clone());
            if table == "doc_mount_points" && !stats_masked.is_empty() {
                mask_stats(&mut g_rows);
                mask_stats(&mut w_rows);
            }
            let g = n_got.value(&Value::Array(g_rows.clone()));
            let wnt = n_want.value(&Value::Array(w_rows.clone()));
            // [P4.147] A shared content row the #141 carve kept: same rows,
            // compared under a canonical order (see FRESH_STORE_SHARED_CONTENT).
            if ((summary_carve.shared_content
                && FRESH_STORE_SHARED_CONTENT.contains(&table.as_str()))
                || (summary_carve.preserve_backfill
                    && PRESERVE_BACKFILL_TABLES.contains(&table.as_str())))
                && partition == "mountIndex"
                && !residual.contains(table.as_str())
            {
                let cg = normalize_canonically(&g_rows, &literals, &shas_got);
                let cw = normalize_canonically(&w_rows, &literals, &shas_want);
                if cg != cw {
                    let detail = cg
                        .iter()
                        .zip(cw.iter())
                        .find(|(a, b)| a != b)
                        .map(|(a, b)| format!("\n    rust:   {a}\n    oracle: {b}"))
                        .unwrap_or_else(|| format!(" row count {} vs {}", cg.len(), cw.len()));
                    failures.push(format!(
                        "[{name}] mountIndex.{table} differs (canonical order — \
                         FRESH_STORE_SHARED_CONTENT / PRESERVE_BACKFILL){detail}"
                    ));
                }
                continue;
            }
            // The documented phase-order residual: same rows, different
            // insertion order. Asserted in both directions instead of diffed.
            if partition == "mountIndex" && residual.contains(table.as_str()) {
                failures.extend(assert_phase_order_residual(
                    name, table, &g_rows, &w_rows, &literals, &shas_got, &shas_want,
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
