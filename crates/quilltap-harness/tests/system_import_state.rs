//! P4.9G4 `.qtap` IMPORT-EXECUTE differential — the **tier-2 DB-state** diff
//! over all four conflict strategies.
//!
//! Both sides start from the SAME committed `system-data-*` fixture bytes and
//! import the SAME payload (the oracle emits the merged all-type export it built
//! from v4's real writer, so the input is provably identical), then every table
//! in all three partitions is dumped row by row and compared, alongside the
//! `executeImport` result body itself. The route arms replay v4's
//! `?action=import-execute` validation surface (missing fields, bad strategy,
//! the `'replace'` → `'overwrite'` remap, the undefined-data TypeError catch)
//! through the dispatch fn.
//!
//! ## Normalization — the restore differential's two origin rules
//!
//! A UUID or ISO timestamp that appears in neither the PRE-import fixture dump
//! nor the payload is minted / write-clock stamped, and is labelled
//! `<minted-N>` / `<ts>` in first-encounter order over a deterministic walk
//! (the result body first, then partitions and tables sorted, rows in rowid
//! order). Everything else — including v4's phantom `duplicate`-arm map ids
//! that ride into memory FKs — is compared exactly (phantoms are minted on both
//! sides at the same walk positions, so their labels line up; the
//! `duplicate` case additionally pins that they dangle).
//!
//! Every per-item warning is compared VERBATIM, SQLite constraint tails and
//! ZodError tails included (P4.148 lifted the `<ENGINE>` mask the quoted
//! families carried — dogfood #140). What stays masked: the `Import failed:`
//! / preflight-wrapper engine tails and the colon families (no oracle row
//! reaches those); the serde-vs-Zod rows are carved, each pinned both ways,
//! by `SERDE_ARM_DIVERGENCES`.
//!
//! ## [P4.33 → bug 11] The former import divergences — CONVERGED
//!
//! Two came from the 2026-08-04 ruling ("import overwrite claims the whole
//! store, and store identity is the ID"), both fixes v5 made FIRST and v4 has
//! since adopted (`3bb664f0`, bug 11):
//!
//! - The overwrite-clear takes `doc_mount_folders` with it, so a re-imported
//!   archive's tree lands clean — pinned on the whole-state arms and the
//!   dedicated `execute_folder_overwrite` arm, now as PLAIN equalities.
//! - A store's identity is its ID, not its display name, and import CREATE
//!   preserves the archive's id — pinned on the four `store_identity_*` arms,
//!   now a plain equality between the two engines' classified end-states.
//!
//! ## `doc_mount_chunks` — the P4.6BK tripwire, CLOSED at unification
//!
//! v4 re-chunks on every database-document write — the payload's own documents
//! AND the managed fields a freshly provisioned character vault / project store
//! / group store receives. When this lane was written, v5 wrote no chunks at
//! all, so the round's shared contract §2 had this family dump the table and
//! assert the gap with a tripwire that FAILED once the gap closed.
//!
//! **P4.6BK closed it in the same round.** The tripwire fired exactly as
//! designed (v5 now mints chunk rows, and `doc_mount_file_links.chunkCount` now
//! agrees), so it is gone: `doc_mount_chunks` is diffed row for row like every
//! other table, and `chunkCount` is compared as an ordinary column. Nothing here
//! is skipped or normalized on that account any more.
//!
//! ## [P4.48] `execute_preserve_ids_unavailable_store_refuses`
//!
//! The preflight's refusal when an existence read FAILS rather than misses. The
//! `orphan-project-store` prep nulls `PROJECT_1`'s `officialMountPointId` on
//! both sides' fixture copies, then a preserveIds import claims that very id:
//! the read succeeds and `applyOverlayOne` throws — the ONE leg where v4
//! propagates too, since only the `_findById` beneath it sits in a
//! `safeQuery(…, null)`.
//!
//! Measured on v4 at `aa464abf`: `success:false`, `warnings: []` (the catch at
//! `execute.ts:483` swallowed the message; only the collision path pushed one),
//! and every partition byte-identical to the baseline. v5 matched all three,
//! which is what proved the fix — v5 used to read the failure as "the id is
//! free" and march on to attempt id-carrying INSERTs into a store it could not
//! read. An EQUALITY arm, not a divergence.
//!
//! **[P4.D91, v4 `275cd7bc`] The silence closed.** v4's bug-79 fix pushes
//! `Import refused before anything was written: <message>` from that same
//! catch, so the arm's `warnings` is no longer empty — it carries the store's
//! unavailability sentence, byte for byte, and a refused-by-collision import
//! now names the collision TWICE (once by the preflight, once wrapped).
//!
//! Mutation-proven: reverting the preflight's Project site to `.ok().flatten()`
//! reddens it on the result body.
//!
//! ## [P4.D91 → bug 79] `execute_named_item_failures`
//!
//! Five items, one per importer that only LOGGED a per-item failure before v4
//! `275cd7bc` gave it a `warnings` entry: a tag, a roleplay template, and the
//! three profile kinds, each with one field of the wrong type so both engines'
//! validation refuses it. No committed archive can express this — every archive
//! is a real export, and a real export imports — so the payload is built to
//! fail. **[P4.143]** v4's five tails are ZodError messages, kept VERBATIM by
//! `mask_warning`'s widened exception; v5's are serde's own sentences, so each
//! warning is a `SERDE_ARM_DIVERGENCES` row — pinned both ways (VANISHED if
//! the tails agree, WRONG SHAPE unless v4's first issue path and v5's serde
//! prefix are the recorded ones), then carved to a stand-in on both sides.
//! (Before P4.143 the quoted-family mask hid all five tails as `<ENGINE>`.)
//!
//! ## [P4.63 → v4 bug 105] `execute_bug105_seed_abort` — CONVERGED at `679e450e3`
//!
//! v4 `e000d6bfc` read `(seeded.provider ?? '').toUpperCase()` at the top of
//! `importConnectionProfiles`' loop body, OUTSIDE the per-item `try`, so a
//! non-string `provider` threw past the loop and aborted the WHOLE import;
//! v5 named the item and carried on (the standing 2026-08-03 ruling). Filed
//! upstream as v4 bug 105; **v4 fixed it at `679e450e3`** (the seeding call
//! moved inside the per-item `try`, the helper type-tests the provider), and
//! the P4.D131 regen measured FULL convergence (drift-ledger §5.4): v4 now
//! answers `success: true`, exactly one warning naming `Bug 105 Connection`,
//! `imported.imageProfiles == 1`, and `main.image_profiles` gains exactly the
//! `Bug 105 Survivor` — byte-for-byte what v5's leg asserted all along. The
//! divergence classifier is retired; the case stays as a PLAIN-EQUALITY
//! regression guard that one malformed profile is named-and-skipped while the
//! import carries on to the importers behind it. **[P4.143]** Its one
//! warning's TAIL is a `SERDE_ARM_DIVERGENCES` row (v4's ZodError vs v5's
//! serde sentence); everything else stays a plain equality.
//!
//! ✅ **The standing vacuity that arm surfaced is CLOSED (P4.70).** The
//! committed `system-data-main.db` used to predate three
//! `connection_profiles` columns — 4.9's `multiCharacterPrefill` (v4 bug 68,
//! `aa464abf`) and 4.10's `fallbackProfileId` / `allowTierFallback` (v4
//! `65f5021c8`) — so EVERY connection-profile import in this family threw on
//! both sides and the arms stayed green on matching failures: the
//! migration-vintage class. Worse, the two engines threw on DIFFERENT columns
//! (v4's `insertOne` names `allowTierFallback` first, because its Zod default
//! makes the key always present; v5's `create` names `fallbackProfileId`, or
//! `multiCharacterPrefill` when the document carries the key) and the
//! (since-retired) `QUOTED_FAMILIES` mask folded both to
//! `Failed to import connection profile "<name>": <ENGINE>` — so even the
//! sentences agreed while nothing was measured.
//!
//! `harness/oracle/fixtures/migrate-system-data-schema.ts` closed all three
//! (and eight more, across `characters` / `chat_settings` / `llm_logs`) by
//! migrating the committed family in place through v4's OWN
//! `compareSchemas` + `generateAlterStatements`. Connection profiles now
//! actually insert: `execute_overwrite_all`, `execute_duplicate_all`,
//! `execute_cross_instance_skip` and `route_replace_remap` carry
//! `connectionProfiles: 1` where they carried 0, and `execute_legacy_folds`
//! carries 2. The create / overwrite / duplicate / skip strategies over
//! connection profiles are differential-covered here for the first time. The
//! P4.D135 understudy remap the `.qtap` reconcile pass performs is NOT: the
//! fixture's one profile row has a NULL `fallbackProfileId` (measured at the
//! `d1c06cd9d` smalls unification), so only the null arm runs here; the
//! non-null remap is pinned by `reconcile.rs`'s
//! `the_understudy_is_remapped_including_a_forward_reference` alone.
//!
//! Two arms change meaning with it, for the better:
//! `execute_named_item_failures`'s `Failed to import connection profile
//! "Broken Connection"` is now a VALIDATION failure, which is the arm the case
//! was built for, rather than the schema failure that satisfied it by
//! accident; and `execute_bug105_seed_abort` no longer rests solely on its
//! image-profile survivor.
//!
//! Generate the oracle (see `system-import-execute.test.ts`), then:
//!   QT_ORACLE_SYSTEM_IMPORT_EXECUTE=/tmp/oracle-system-import-execute.ndjson \
//!     cargo test -p quilltap-harness --test system_import_state -- --nocapture

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};

use quilltap_core::api::system_qtap;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::file_storage::NotConfiguredPixelCodec;
use quilltap_core::services::quilltap_import::{
    execute_import, ConflictStrategy, ImportOptions, PreserveIdsMode, QuilltapExport,
};
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// A whole-instance dump: partition → table → rows.
type StateDump = BTreeMap<String, BTreeMap<String, Vec<Value>>>;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../quilltap-web/tests/fixtures")
}

struct Scratch {
    root: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fresh_fixture(tag: &str) -> Scratch {
    let root = std::env::temp_dir().join(format!("qt-importstate-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    for (src, dst) in [
        ("system-data-main.db", "main.db"),
        ("system-data-mount.db", "mount.db"),
        ("system-data-llmlogs.db", "llm.db"),
    ] {
        std::fs::copy(fixtures_dir().join(src), root.join(dst)).unwrap();
    }
    // P4.111 widened the committed `system-data-main.db` to carry the
    // P4.D171/P4.D182 columns natively (`pragma_table_info` proof: P4.117
    // lane record) — the `ensure_*_column[s]` heals that used to run here
    // are dead and removed; the value PLANTS stay (the non-default cells
    // this family exists to compare).
    //
    // ⚠ P4.88: the plant belongs HERE, on the fixture copy, not in
    // `open_db`. The route-validation arms read their `pre` state BEFORE opening
    // the Db, so anything `open_db` writes reads as "a validation-failure arm
    // MUTATED the database". The oracle's own `preState` is taken after its
    // plant too, so the two engines start from the same cells either way.
    {
        let w =
            quilltap_core::db::Writer::open_writable(&root.join("main.db"), TEST_PEPPER).unwrap();
        plant_p4d171_values(w.connection());
        // [P4.106 item 6] The committed triple predates Inform (`e7d77bb60`) too.
        // A live v5 instance has `chat_informs` before any import runs — the
        // boot chain's `ensure_chat_informs_table` — while v4's repository
        // creates the collection lazily on its first write. Without this the
        // import's inform inserts fail `no such table` on the vintage copy (an
        // instrument gap, measured: the first run of the three
        // `execute_chats_informs_*` arms). An absent table and an empty one
        // compare equal in `diff_states`, so every other arm is unmoved.
        quilltap_core::db::chat_informs::ensure_chat_informs_table(w.connection())
            .expect("ensure chat_informs on the vintage fixture");
        let touched = w
            .connection()
            .execute(
                "UPDATE \"files\" SET \"generationKey\" = ?1 WHERE \"id\" = ?2",
                rusqlite::params![
                    "a3000000-0000-4000-8000-000000000001",
                    "f0000001-0000-4000-8000-000000000001"
                ],
            )
            .expect("plant the avatar cache key");
        assert_eq!(touched, 1, "the planted file row must exist in the fixture");
    }
    Scratch { root }
}

fn open_db(scratch: &Scratch) -> Db {
    Db::open(
        DbPaths {
            main: scratch.root.join("main.db"),
            mount_index: Some(scratch.root.join("mount.db")),
            llm_logs: Some(scratch.root.join("llm.db")),
        },
        TEST_PEPPER,
    )
    .expect("open fixture instance")
}

/// Dump one partition, every table, rowid order; BLOBs → `sha256:<hex>`;
/// integral REALs canonicalized to integers (the JS-number dump artifact).
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

fn read_state(scratch: &Scratch) -> StateDump {
    let mut out = BTreeMap::new();
    for (label, file) in [
        ("main", "main.db"),
        ("mountIndex", "mount.db"),
        ("llmLogs", "llm.db"),
    ] {
        let conn = quilltap_core::db::Writer::open_writable(&scratch.root.join(file), TEST_PEPPER)
            .expect("reopen partition");
        out.insert(label.to_string(), dump_partition(conn.connection()));
    }
    out
}

/// Oracle-side state (`{main:{table:[rows]}, …}`) into the same BTreeMap shape.
fn state_from_value(v: &Value) -> StateDump {
    let mut out = BTreeMap::new();
    for (part, tables) in v.as_object().expect("state is an object") {
        let mut t = BTreeMap::new();
        for (name, rows) in tables.as_object().expect("partition is an object") {
            t.insert(name.clone(), rows.as_array().cloned().unwrap_or_default());
        }
        out.insert(part.clone(), t);
    }
    out
}

// ── the origin-based normalizer (the restore differential's two rules) ──────

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

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

fn collect_strings(v: &Value, out: &mut HashSet<String>) {
    match v {
        Value::String(s) => {
            if is_uuid(s) || is_iso(s) {
                out.insert(s.clone());
            }
            // UUIDs / timestamps EMBEDDED in longer strings (message content, a
            // vault document body riding the payload) are origin literals too.
            if s.len() > 36 {
                scan_embedded(s, out);
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect_strings(x, out)),
        _ => {}
    }
}

fn scan_embedded(s: &str, out: &mut HashSet<String>) {
    let b = s.as_bytes();
    let mut i = 0usize;
    while i + 24 <= b.len() {
        if let Ok(w24) = std::str::from_utf8(&b[i..i + 24]) {
            if is_iso(w24) {
                out.insert(w24.to_string());
            }
        }
        if i + 36 <= b.len() {
            if let Ok(w36) = std::str::from_utf8(&b[i..i + 36]) {
                if is_uuid(w36) {
                    out.insert(w36.to_string());
                }
            }
        }
        i += 1;
    }
}

/// Canonicalize a JSON-TEXT column (parse, drop null-valued object keys,
/// re-emit) — applied to BOTH sides; see `system_restore_state.rs` for the
/// documented cost (absent-vs-null inside a JSON column is invisible).
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

/// The content hashes of `doc_mount_documents` rows whose CONTENT carries a
/// minted id or a write-clock stamp — a wardrobe item's YAML front matter holds
/// its own freshly-minted id, a project's `properties.json` holds the remapped
/// roster. Such a hash is nondeterministic across engines, and unlike the
/// restore differential's case it lives in a DIFFERENT table from the content
/// (`doc_mount_files.sha256`), so the per-object "did a composite change?"
/// heuristic cannot see it. Computed up front per side and masked everywhere the
/// value appears; the CONTENT itself is still compared, normalized.
fn derived_hashes(state: &StateDump, literals: &HashSet<String>) -> HashSet<String> {
    let empty = Vec::new();
    let mut out = HashSet::new();
    for row in state
        .get("mountIndex")
        .and_then(|t| t.get("doc_mount_documents"))
        .unwrap_or(&empty)
    {
        let Some(content) = row.get("content").and_then(Value::as_str) else {
            continue;
        };
        let mut found = HashSet::new();
        scan_embedded(content, &mut found);
        if found.iter().any(|f| !literals.contains(f)) {
            if let Some(sha) = row.get("contentSha256").and_then(Value::as_str) {
                out.insert(sha.to_string());
            }
        }
    }
    out
}

// ── [P4.33 → bug 11] the overwrite-clear + store-identity divergences RETIRED ──
//
// v5 made both fixes first (a store's identity is its ID; overwrite means
// overwrite, clearing folders too), and v4 has since CONVERGED (`3bb664f0`,
// bug 11: `import-document-stores.ts` matches `byId`, `preserveArchiveId`s on
// create, and clears folders on overwrite). So the whole-state arms
// (`execute_overwrite_all`, `route_replace_remap`, `execute_link_groups_twice`)
// are now PLAIN equalities — the folder-clear / store-create divergence
// machinery and its FOLDER_CLEAR_DIVERGENCE / STORE_ID_PRESERVED_ON_CREATE
// carve-outs are gone. The `store_identity_*` and `execute_folder_overwrite`
// arms assert the converged behavior directly (see their runners).

// (The P4.D51-round `ANNOTATION_SWEEP_PENDING_P4D53` cross-lane carve-out lived
// here: the import overwrite path deletes chats through `chats::delete`, and
// until P4.D53's per-chat annotation sweep landed there, v5 kept husks v4's
// bug-10 fix removed. The carve-out's tripwire fired at the f4955e0e-round
// unification exactly as designed (v5 3 vs v4 3 on both flagged arms) and was
// retired — `main.conversation_annotations` is a plain equality again.)

struct Normalizer {
    literals: HashSet<String>,
    derived_hashes: HashSet<String>,
    /// [P4.33] Structural ids whose VALUE is deliberately incomparable across
    /// engines, mapped to a semantic key instead of a walk-order `<minted-N>`
    /// label — see [`folder_labels`]. Consulted before both `literals` and the
    /// minting counter, so a side that writes a fresh row where the other keeps
    /// an old one cannot shift every later label.
    id_labels: BTreeMap<String, String>,
    minted: BTreeMap<String, String>,
}

impl Normalizer {
    fn new(literals: HashSet<String>) -> Self {
        Normalizer {
            literals,
            derived_hashes: HashSet::new(),
            id_labels: BTreeMap::new(),
            minted: BTreeMap::new(),
        }
    }

    fn with_derived_hashes(mut self, hashes: HashSet<String>) -> Self {
        self.derived_hashes = hashes;
        self
    }

    fn with_id_labels(mut self, labels: BTreeMap<String, String>) -> Self {
        self.id_labels = labels;
        self
    }

    fn substitute_embedded(&mut self, s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0usize;
        while i < b.len() {
            if i + 36 <= b.len() {
                if let Ok(w) = std::str::from_utf8(&b[i..i + 36]) {
                    if let Some(label) = self.id_labels.get(w) {
                        out.push_str(label);
                        i += 36;
                        continue;
                    }
                    if is_uuid(w) && !self.literals.contains(w) {
                        let next = self.minted.len();
                        let label = self
                            .minted
                            .entry(w.to_string())
                            .or_insert_with(|| format!("<minted-{next}>"));
                        out.push_str(label);
                        i += 36;
                        continue;
                    }
                }
            }
            if i + 24 <= b.len() {
                if let Ok(w) = std::str::from_utf8(&b[i..i + 24]) {
                    if is_iso(w) && !self.literals.contains(w) {
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
                if let Some(label) = self.id_labels.get(s) {
                    return Value::String(label.clone());
                }
                if self.derived_hashes.contains(s) {
                    return Value::String("<sha:derived-from-normalized>".to_string());
                }
                if self.literals.contains(s) {
                    return v.clone();
                }
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
                if s.len() > 24 {
                    let sub = self.substitute_embedded(s);
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
                    if let (Value::String(before), Value::String(after)) = (val, &nv) {
                        if before != after && !is_uuid(before) && !is_iso(before) {
                            composite_normalized = true;
                        }
                    }
                    out.insert(k.clone(), nv);
                }
                // A content hash over text that itself had to be normalized holds
                // no comparable information — mask ONLY then (see the restore
                // differential's rationale).
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

// ── warning masking (the engine-wording seam) ───────────────────────────────

/// Colon-delimited families: keep the prefix, mask the rest.
const COLON_FAMILIES: &[&str] = &[
    "Failed to import memory: ",
    "Failed to import conversation annotation: ",
    "Failed to import chat document: ",
    "Failed to reconcile character relationships: ",
    "Failed to reconcile chat relationships: ",
    "Failed to reconcile project relationships: ",
    "Failed to reconcile connection profile relationships: ",
    "Failed to reconcile image profile relationships: ",
    "Failed to reconcile embedding profile relationships: ",
    "Failed to reconcile roleplay template relationships: ",
];

/// [P4.130 Tier 2 item 10 → P4.143 item 1] The import's serde-decode arms — a
/// RECORDED DIVERGENCE per row, each pinned in both directions.
///
/// v4 refuses a malformed item at its repository create's Zod `validate`
/// (`base.repository.ts:130-141`), so the skip warning's tail is the ZodError
/// message — kept VERBATIM by `mask_warning`'s exception in every quoted
/// family. v5 decodes the item into a typed DTO first (`serde_json::from_value`)
/// and answers serde's own `Display` sentence, which carries no path. Both
/// skip the item and carry on; only the warning's tail differs.
///
/// - The CHAT row (P4.130): a chat whose three Concierge columns pass but whose
///   `scenarioText` is a number — v5 checks only the Concierge columns before
///   its typed decode (`concierge_columns_zod_error`'s recorded scope).
/// - The SIX create rows (P4.143, measured at `f6426e196`): the tag
///   (`visualStyle: "not-an-object"` — v4's `invalid_type` expected object,
///   v5's `expected struct TagVisualStyle`), and the five integer fields
///   (`modelName` / `provider` × 3 / `systemPrompt` = 42 — v4's `invalid_type`
///   expected string, or for the embedding provider the `invalid_value` over
///   `EmbeddingProfileProviderEnum`'s five values; v5's ``invalid type:
///   integer `42`, expected a string``).
///
/// Closing them needs a GENERATED schema-shape table (P4.143 Tier 3 item 12);
/// a row retires by measurement when its two tails agree ("VANISHED").
struct SerdeArm {
    /// The oracle case the row lives in.
    case: &'static str,
    /// The warning's deterministic head, through `": "`.
    head: &'static str,
    /// v4's tail must be a Zod message whose FIRST issue's `path` is
    /// `[v4_path_key]`.
    v4_path_key: &'static str,
    /// v5's tail must start with serde's sentence.
    v5_serde_prefix: &'static str,
}

const SERDE_ARM_DIVERGENCES: &[SerdeArm] = &[
    SerdeArm {
        case: "execute_concierge_serde_arm",
        head: "Failed to import chat \"Concierge Bogus 8\": ",
        v4_path_key: "scenarioText",
        v5_serde_prefix: "invalid type: integer `5`, expected a string",
    },
    // [P4.161 Tier 2 item 11] The `Broken Tag` row RETIRED: the tag import
    // now validates through `TagSchema` (`parse_create_tag`), so its tail is
    // v4's ZodError bytes and the whole-state diff compares it plainly.
    SerdeArm {
        case: "execute_named_item_failures",
        head: "Failed to import connection profile \"Broken Connection\": ",
        v4_path_key: "modelName",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
    SerdeArm {
        case: "execute_named_item_failures",
        head: "Failed to import image profile \"Broken Image\": ",
        v4_path_key: "provider",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
    SerdeArm {
        case: "execute_named_item_failures",
        head: "Failed to import embedding profile \"Broken Embedding\": ",
        v4_path_key: "provider",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
    SerdeArm {
        case: "execute_named_item_failures",
        head: "Failed to import roleplay template \"Broken Template\": ",
        v4_path_key: "systemPrompt",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
    SerdeArm {
        case: "execute_bug105_seed_abort",
        head: "Failed to import connection profile \"Bug 105 Connection\": ",
        v4_path_key: "provider",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
    // [P4.143 Tier 2 item 7] The image / embedding `duplicate` arms — named on
    // both sides since P4.143 (v5's used to drop the item silently); their
    // tails are the same serde-vs-Zod class.
    SerdeArm {
        case: "execute_duplicate_malformed_profiles",
        head: "Failed to import image profile \"Duplicate Broken Image\": ",
        v4_path_key: "provider",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
    SerdeArm {
        case: "execute_duplicate_malformed_profiles",
        head: "Failed to import embedding profile \"Duplicate Broken Embedding\": ",
        v4_path_key: "modelName",
        v5_serde_prefix: "invalid type: integer `42`, expected a string",
    },
];

/// How many `SERDE_ARM_DIVERGENCES` rows the run classified — asserted equal
/// to the table's length after the loop, so a row whose case or head never
/// appears is a FAILURE, never a silent skip.
static SERDE_ARMS_EXERCISED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Apply every `SERDE_ARM_DIVERGENCES` row of case `name`: VANISHED if the two
/// tails agree, WRONG SHAPE unless v4's tail is a Zod message whose first
/// issue's path is `[v4_path_key]` AND v5's starts with `v5_serde_prefix`;
/// then both sides' warning is replaced with `<head><SERDE-ARM-DIVERGENCE>`
/// before the result compare.
fn classify_serde_arms(
    name: &str,
    got: &Value,
    want: &Value,
    failures: &mut Vec<String>,
) -> (Value, Value) {
    let (mut got, mut want) = (got.clone(), want.clone());
    for arm in SERDE_ARM_DIVERGENCES.iter().filter(|a| a.case == name) {
        SERDE_ARMS_EXERCISED.fetch_add(1, Ordering::SeqCst);
        let tail_of = |body: &Value| -> Option<String> {
            body["warnings"]
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .find_map(|w| w.strip_prefix(arm.head).map(str::to_string))
        };
        let head = arm.head;
        let (g, w) = (tail_of(&got), tail_of(&want));
        let first_path_is = |tail: &str| {
            serde_json::from_str::<Value>(tail)
                .ok()
                .and_then(|v| v.get(0).and_then(|i| i.get("path")).cloned())
                == Some(json!([arm.v4_path_key]))
        };
        match (&g, &w) {
            (Some(g), Some(w)) if g == w => failures.push(format!(
                "[{name}] {head:?}: the serde-arm divergence VANISHED — both sides now say \
                 {g:?}; retire the row"
            )),
            (Some(g), Some(w))
                if is_zod_error_message(w)
                    && first_path_is(w)
                    && g.starts_with(arm.v5_serde_prefix) => {}
            _ => failures.push(format!(
                "[{name}] {head:?}: the serde-arm divergence has the WRONG SHAPE\n  rust:   \
                 {g:?}\n  oracle: {w:?}"
            )),
        }
        let carve = |body: &mut Value| {
            if let Some(ws) = body.get_mut("warnings").and_then(Value::as_array_mut) {
                for w in ws.iter_mut() {
                    if w.as_str().is_some_and(|s| s.starts_with(head)) {
                        *w = json!(format!("{head}<SERDE-ARM-DIVERGENCE>"));
                    }
                }
            }
        };
        carve(&mut got);
        carve(&mut want);
    }
    (got, want)
}

// ── [P4.143 Tier 2 item 10] the refused chat create's repository lines ──────

/// The messages v4's refused chat create logs (its oracle's
/// `REPO_LOG_MESSAGES`): the THREE repository ERRORs and the per-chat WARN —
/// and, since P4.155 (R-A), the refused project / group create's three
/// ERRORs (`Data validation failed`, the store-backed `_create` override's
/// `Error creating {label} entity`, the store-backed `create` wrap's `Error
/// creating {label}`) and its per-item WARN.
const REPO_LOG_MESSAGES: &[&str] = &[
    "Data validation failed",
    "Error creating entity",
    "Failed to create chat",
    "Failed to import chat",
    "Error creating project entity",
    "Error creating group entity",
    "Error creating project",
    "Error creating group",
    "Failed to import project",
    "Failed to import group",
    // [P4.161] The refused memory create's third ERROR — the memories
    // repository's own wrap (`memories.repository.ts:418-428`).
    "Error creating memory",
    // [P4.161 Tier 2] the prompt-templates repository's wrap.
    "Error creating prompt template",
    "Error creating folder",
    "Error creating tag",
    "Error creating file",
];

/// The [`REPO_LOG_MESSAGES`] entry `rest` starts with — the LONGEST match, so
/// `Error creating project entity …` is never read as `Error creating
/// project` with a stray `entity` token.
fn repo_log_message(rest: &str) -> Option<&'static str> {
    REPO_LOG_MESSAGES
        .iter()
        .copied()
        .filter(|m| rest.starts_with(&format!("{m} ")))
        .max_by_key(|m| m.len())
}

/// The context keys a recorded repository line may carry ahead of `error`, in
/// the oracle's key list. A value runs to the next known key — `name=` carries
/// the project's name, spaces and all (`%` renders it unquoted).
const REPO_LOG_KEYS: &[&str] = &[
    "collection",
    "chatId",
    "characterId",
    "userId",
    "name",
    "path",
    "filename",
    "projectId",
    "groupId",
];

/// v5's captured lines (`LEVEL target message k=v …`, the `CaptureLayer`
/// shape) projected onto the oracle's `{level, message, collection, chatId,
/// name, projectId, groupId, error}` record. `error` is every line's LAST
/// field, so it runs to the end of the line (a ZodError message spans many).
fn v5_repo_logs(lines: &[String]) -> Vec<Value> {
    let mut out = Vec::new();
    for line in lines {
        let mut parts = line.splitn(3, ' ');
        let (Some(level), Some(_target), Some(rest)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(message) = repo_log_message(rest) else {
            continue;
        };
        let fields = &rest[message.len()..];
        let mut rec = serde_json::Map::new();
        rec.insert("level".into(), json!(level.to_lowercase()));
        rec.insert("message".into(), json!(message));
        let (head, error) = fields.split_once(" error=").unwrap_or((fields, ""));
        let mut starts: Vec<(usize, &str)> = REPO_LOG_KEYS
            .iter()
            .filter_map(|k| head.find(&format!(" {k}=")).map(|i| (i, *k)))
            .collect();
        starts.sort();
        for (n, (i, k)) in starts.iter().enumerate() {
            let from = i + k.len() + 2;
            let to = starts.get(n + 1).map_or(head.len(), |(j, _)| *j);
            rec.insert(k.to_string(), json!(&head[from..to]));
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
/// agree). `strictFailures` is pinned on BOTH sides to exactly the lines
/// `strict_lines` names (the import's two `safeQuery`-born ERRORs), then
/// dropped from the compare — v5's import entered the strict-repository scope
/// at the `f6426e196` recorded-divergences unification, and the chat-create
/// helper reads it, so the field is no longer v4-only (it was P4.143's pinned
/// v4-only field until then).
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
    // v5's field rides LAST (after `error`, which the parser reads to the end
    // of the line): pin it on exactly `strict_lines`, then strip it.
    let mut stripped: Vec<String> = Vec::with_capacity(got_lines.len());
    for l in got_lines {
        let is_strict = l.ends_with(" strictFailures=true");
        let line = l.strip_suffix(" strictFailures=true").unwrap_or(l);
        if let Some(message) = line.splitn(3, ' ').nth(2).and_then(repo_log_message) {
            if is_strict != strict_lines.contains(&message) {
                failures.push(format!(
                    "[{name}] v5 `strictFailures` on {message:?}: {is_strict}, expected {}",
                    strict_lines.contains(&message)
                ));
            }
        }
        stripped.push(line.to_string());
    }
    let got = v5_repo_logs(&stripped);
    let mut want: Vec<Value> = want.clone();
    for w in want.iter_mut() {
        let message = w["message"].as_str().unwrap_or("").to_string();
        let strict = w.as_object_mut().and_then(|o| o.remove("strictFailures"));
        // [P4.161 Tier 2] A non-string context value (a tag's numeric `name`)
        // renders as v5's capture renders it — its JSON text.
        if let Some(o) = w.as_object_mut() {
            for v in o.values_mut() {
                if !v.is_string() {
                    *v = json!(v.to_string());
                }
            }
        }
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

/// [P4.148 Tier 2 item 17] The nine WARNs v4's import logs that v5 had no twin
/// for (the oracle's `IMPORT_WARN_MESSAGES`), each `"<message> k=v …"`.
const IMPORT_WARN_MESSAGES: &[&str] = &[
    "Failed to import wardrobe item",
    "Failed to import prompt template",
    "Failed to import provider model",
    "Failed to import plugin config",
    "Failed to import instance setting",
    "Failed to import folder",
    "Failed to import file",
    "Failed to import memory",
    "Imported memories left unembedded",
];

/// v5's captured WARN lines (`WARN <target> <message> k=v …`) for those
/// messages, with the level and target dropped — the oracle's string shape.
fn v5_import_warns(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| {
            let rest = l.strip_prefix("WARN ")?;
            let (_target, rest) = rest.split_once(' ')?;
            IMPORT_WARN_MESSAGES
                .iter()
                .any(|m| rest == *m || rest.starts_with(&format!("{m} ")))
                .then(|| rest.to_string())
        })
        .collect()
}

/// How many cases compared the nine import WARNs, and how many lines v4 fired
/// across them — asserted after the run (non-vacuity: the two firing cases).
static IMPORT_WARN_CASES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static IMPORT_WARNS_FIRED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many cases compared `repoLogs` — asserted after the run (the two chat
/// refusal arms), so the pin cannot go vacuous by an oracle that stopped
/// recording.
static REPO_LOG_CASES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

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

fn mask_warning(w: &str) -> String {
    // Fully deterministic warnings stay verbatim.
    if w.starts_with("Memory references non-existent character ") {
        return w.to_string();
    }
    // [P4.D91 → bug 79] The preflight's refusal wrapper. Its tail is the
    // preflight's own message, which is deterministic for a collision and for
    // the store-unavailable sentence — both are compared VERBATIM. Anything
    // else is an engine sentence and is masked.
    if let Some(rest) = w.strip_prefix("Import refused before anything was written: ") {
        if rest.starts_with("Preserve IDs collision for ") || rest.contains("has no usable") {
            return w.to_string();
        }
        return "Import refused before anything was written: <ENGINE>".to_string();
    }
    if w.starts_with("Import failed: Cannot read properties") {
        return w.to_string();
    }
    if let Some(rest) = w.strip_prefix("Import failed: ") {
        let _ = rest;
        return "Import failed: <ENGINE>".to_string();
    }
    // [P4.148 → dogfood #140] The quoted families (`Failed to import folder
    // "…": `, `… message in chat "…": `, the five P4.D91 arms, …) and
    // `Failed to link project … to mount point …: ` are compared VERBATIM —
    // the `<ENGINE>` mask that used to fold their tails is GONE. v5's import
    // renders every `DbError` through `item_error_text` (the bare SQLite
    // sentence, as v4's `error.message`), and both engines run v4's DDL, so
    // the constraint sentences agree byte for byte; a ZodError tail was
    // already kept verbatim (P4.143). Red-first on unported `main` at
    // `07b8f0209`: the six cases carrying a SQLite tail
    // (`execute_skip_all`, `execute_overwrite_all`, `execute_duplicate_all`,
    // `execute_cross_instance_skip`, `route_replace_remap`,
    // `execute_chats_informs_duplicate`) on the `sqlite error: ` prefix.
    // `SERDE_ARM_DIVERGENCES` still carves the serde-vs-Zod rows. The colon
    // families below stay masked: no oracle row reaches one, so lifting them
    // would prove nothing.
    for family in COLON_FAMILIES {
        if w.starts_with(family) {
            return format!("{family}<ENGINE>");
        }
    }
    w.to_string()
}

fn mask_result_warnings(result: &Value) -> Value {
    let mut out = result.clone();
    if let Some(warnings) = out.get_mut("warnings").and_then(Value::as_array_mut) {
        for w in warnings.iter_mut() {
            if let Some(s) = w.as_str() {
                *w = Value::String(mask_warning(s));
            }
        }
    }
    out
}

// ── case plumbing ───────────────────────────────────────────────────────────

fn read_cases() -> Option<Vec<Value>> {
    let path = std::env::var("QT_ORACLE_SYSTEM_IMPORT_EXECUTE").ok()?;
    let raw = std::fs::read_to_string(&path).expect("read oracle ndjson");
    Some(
        raw.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
            .collect(),
    )
}

fn export_of(export_data: &Value) -> QuilltapExport {
    QuilltapExport {
        manifest: export_data.get("manifest").cloned().unwrap_or(Value::Null),
        data: export_data.get("data").cloned().unwrap_or(Value::Null),
    }
}

/// v4's `preserveIdsMode` bag, as the oracle emits it:
/// `{"mode":"skip-if-present","targetCharacterId":…,"targetVaultMountPointId":…}`
/// or absent (= `refuse-on-collision`).
fn preserve_ids_mode_of(options: &Value) -> PreserveIdsMode {
    let Some(mode) = options.get("preserveIdsMode") else {
        return PreserveIdsMode::RefuseOnCollision;
    };
    match mode.get("mode").and_then(Value::as_str) {
        Some("skip-if-present") => PreserveIdsMode::SkipIfPresent {
            target_character_id: mode
                .get("targetCharacterId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            target_vault_mount_point_id: mode
                .get("targetVaultMountPointId")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        _ => PreserveIdsMode::RefuseOnCollision,
    }
}

fn options_of(options: &Value) -> ImportOptions {
    let strategy = match options.get("conflictStrategy").and_then(Value::as_str) {
        Some("skip") => ConflictStrategy::Skip,
        Some("overwrite") | Some("replace") => ConflictStrategy::Overwrite,
        Some("duplicate") => ConflictStrategy::Duplicate,
        other => panic!("oracle execute case with unexpected strategy {other:?}"),
    };
    ImportOptions {
        preserve_ids: options
            .get("preserveIds")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        preserve_ids_mode: preserve_ids_mode_of(options),
        conflict_strategy: strategy,
        include_memories: options
            .get("includeMemories")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        include_related_entities: false,
        selected_ids: options.get("selectedIds").cloned(),
    }
}

/// The literal set: every UUID/timestamp in the PRE-import fixture dump plus
/// the payload (and the options bag, which is id-free in practice).
fn literals_for(pre_state: &StateDump, payload: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    for tables in pre_state.values() {
        for rows in tables.values() {
            for row in rows {
                collect_strings(row, &mut out);
            }
        }
    }
    collect_strings(payload, &mut out);
    out
}

/// Normalize a whole (result, state) pair with ONE normalizer so minted labels
/// are consistent between the result body and the row dumps.
fn normalize_side(
    literals: &HashSet<String>,
    derived: HashSet<String>,
    result: &Value,
    state: &StateDump,
    id_labels: BTreeMap<String, String>,
) -> (Value, StateDump) {
    let mut n = Normalizer::new(literals.clone())
        .with_derived_hashes(derived)
        .with_id_labels(id_labels);
    let norm_result = n.value(&mask_result_warnings(result));
    let mut norm_state = BTreeMap::new();
    for (part, tables) in state {
        let mut t = BTreeMap::new();
        for (table, rows) in tables {
            let v = n.value(&Value::Array(rows.clone()));
            t.insert(table.clone(), v.as_array().cloned().unwrap_or_default());
        }
        norm_state.insert(part.clone(), t);
    }
    (norm_result, norm_state)
}

// **P4.D183 retired P4.D182's `subtract_the_deferred_transcript_counter`.**
//
// The `31436bae4` round split `chats.transcriptVersion` across two lanes:
// P4.D182 gave the column its boot ensure, P4.D183 its single writer. In
// between, an import that added messages left v4's counter at the number of
// message writes and v5's at 0 — a real difference this family could see,
// so P4.D182 recorded it as a tripwire rather than a subtraction.
//
// The tripwire FIRED at P4.D183's first regen (`execute_overwrite_all`, v5 at
// 2 where the assertion demanded 0) and this is what that firing buys: the
// column now diffs PLAINLY, cell for cell, on every arm. That is a stronger
// claim than either half — it is a differential of v5's bump against v4's
// over the whole import corpus, and it costs nothing, because the funnel is
// already the thing both sides drive.

/// The retired tripwire's one surviving obligation: keep the plain diff from
/// going vacuous.
///
/// `transcriptVersion` diffs like any other column now, which means it also
/// AGREES trivially when neither engine moves it — and 24 of the corpus's 27
/// chat-carrying arms import no message at all, so zero-vs-zero is the common
/// case. Three arms do move it (`execute_overwrite_all`,
/// `execute_cross_instance_skip`, `route_replace_remap`, all at 2). This
/// accumulates the highest counter the RUST side produced and asserts once
/// that it is above zero.
///
/// Deliberately the rust side, not the oracle's: an assertion on the oracle
/// cannot catch a v5 regression (`an-assertion-on-the-oracle-cannot-catch-a-v5-
/// regression`). If v5's funnel ever stopped bumping, every arm would still
/// diff clean against a v4 that bumps — because the subtraction is gone and
/// the rows would simply both read 0 if the fixture stopped importing
/// messages. This is what says otherwise.
static V5_TRANSCRIPT_MAX: AtomicI64 = AtomicI64::new(0);

fn record_v5_transcript_counters(got: &StateDump) {
    let mut max = 0i64;
    for tables in got.values() {
        let Some(rows) = tables.get("chats") else {
            continue;
        };
        for row in rows {
            if let Some(v) = row.get("transcriptVersion").and_then(|v| v.as_i64()) {
                max = max.max(v);
            }
        }
    }
    V5_TRANSCRIPT_MAX.fetch_max(max, Ordering::Relaxed);
}

fn diff_states(name: &str, got: &StateDump, want: &StateDump, failures: &mut Vec<String>) {
    let all_tables: HashSet<(String, String)> = got
        .iter()
        .flat_map(|(p, t)| t.keys().map(move |k| (p.clone(), k.clone())))
        .chain(
            want.iter()
                .flat_map(|(p, t)| t.keys().map(move |k| (p.clone(), k.clone()))),
        )
        .collect();
    let mut sorted: Vec<_> = all_tables.into_iter().collect();
    sorted.sort();
    for (part, table) in sorted {
        let empty = Vec::new();
        let g = got.get(&part).and_then(|t| t.get(&table)).unwrap_or(&empty);
        let w = want
            .get(&part)
            .and_then(|t| t.get(&table))
            .unwrap_or(&empty);
        if g != w {
            let detail = if g.len() != w.len() {
                format!("row count: rust {} vs oracle {}", g.len(), w.len())
            } else {
                g.iter()
                    .zip(w.iter())
                    .enumerate()
                    .find(|(_, (a, b))| a != b)
                    .map(|(i, (a, b))| format!("row {i}:\n    rust:   {a}\n    oracle: {b}"))
                    .unwrap_or_default()
            };
            failures.push(format!("[{name}] {part}.{table} differs\n  {detail}"));
        }
    }
}

// ── [P4.33] the ruled store-IDENTITY divergence ─────────────────────────────

/// The two ids the `store_identity_*` archives carry, mirrored from
/// `system-import-execute.test.ts`. Every arm asserts they actually appear in
/// its emitted steps, so an edit on the oracle side cannot silently desync them
/// and leave the classification calling everything `minted`.
const IDENTITY_ID_1: &str = "af1d0000-0000-4000-8000-000000000001";
const IDENTITY_ID_2: &str = "af1d0000-0000-4000-8000-000000000002";

/// ## Store identity is the ID (P4.33 ruling; v4 CONVERGED in bug 11, `3bb664f0`)
///
/// v4 used to match an archive's store to the instance by NAME
/// (`import-document-stores.ts:55-57`) and mint a fresh id on create (`:85-105`),
/// so an archive could never be re-recognized by identity: it claimed whatever
/// store wore its name today, and a rename on either side redirected it onto a
/// stranger. The 2026-08-04 ruling made the id the identity, the name display
/// only, and had import CREATE preserve the archive's id — a fix v5 made first
/// (`services::quilltap_import::document_stores::import_document_stores`). v4 has
/// since adopted it (`import-document-stores.ts`: `byId` + `preserveArchiveId`),
/// so the two engines now produce the SAME classified end-state, and the
/// `store_identity_*` arms are PLAIN equalities.
///
/// The dump is `stores` (store class + name) and `docs` (which store each
/// document landed in), the classes being `archive-1` / `archive-2` (the id the
/// payload carried) / `minted` — which is exactly the claim, stated as data:
/// *did the import land on the store the archive names, or on a new one?* Each
/// arm asserts at least one store wears a preserved archive id, so a
/// classification that silently called everything `minted` (an oracle-side id
/// drift) cannot make the equality vacuous.
///
/// **Two consequences worth knowing.** `store_identity_same_name_new_id` ends
/// with two stores both named `Identity Store`: the overwrite branch writes the
/// archive's name onto the store it matched by id. Nothing in this port
/// uniquifies a name on UPDATE, and the ruling speaks only of CREATE, so the
/// duplicate stands — a tolerated state, not a corruption (store names have no
/// unique index; `doc_edit::uri_producers` falls back to the UUID form when
/// `count_by_name > 1`; `db::mount_index_case_repair` renames the loser on the
/// next boot). And `store_identity_skip_by_id` shows `skip` is not a no-op for a
/// recognized store: both engines pour the archive's documents into whatever
/// store the id map points at.
///
/// Fold one side's raw dump into the classified shape. `pre` is the PRE-import
/// store id set, so a fixture store that
/// somehow matched the name filter would be named rather than called `minted`.
fn classify_identity_dump(stores: &[Value], docs: &[Value], pre: &HashSet<String>) -> Value {
    let class = |id: &str| -> &'static str {
        match id {
            IDENTITY_ID_1 => "archive-1",
            IDENTITY_ID_2 => "archive-2",
            other if pre.contains(other) => "pre-existing",
            _ => "minted",
        }
    };
    json!({
        "stores": stores
            .iter()
            .map(|s| json!({
                "id": class(s["id"].as_str().unwrap_or_default()),
                "name": s["name"].clone(),
            }))
            .collect::<Vec<_>>(),
        "docs": docs
            .iter()
            .map(|d| json!({
                "store": class(d["storeId"].as_str().unwrap_or_default()),
                "storeName": d["storeName"].clone(),
                "relativePath": d["relativePath"].clone(),
            }))
            .collect::<Vec<_>>(),
    })
}

/// Replay a `store_identity_*` arm's steps through the Rust engine and check the
/// v5 end-state equals v4's — a PLAIN equality since v4 converged (bug 11).
fn run_store_identity_case(name: &str, case: &Value, user_id: &str, failures: &mut Vec<String>) {
    let steps = case["steps"].as_array().cloned().unwrap_or_default();

    let scratch = fresh_fixture(name);
    let pre_ids: HashSet<String> = read_state(&scratch)
        .get("mountIndex")
        .and_then(|t| t.get("doc_mount_points"))
        .map(|rows| {
            rows.iter()
                .filter_map(|r| r.get("id").and_then(Value::as_str))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let db = open_db(&scratch);
    let uid = user_id.to_string();
    let replay = steps.clone();
    db.write_blocking(move |ws| {
        let main = ws.main().connection();
        let mount = ws.mount_index().expect("fixture has a mount partition");
        for step in &replay {
            match step["op"].as_str().unwrap_or_default() {
                "import" => {
                    let export = export_of(&step["data"]);
                    let opts = options_of(&step["options"]);
                    execute_import(
                        main,
                        mount.connection(),
                        &uid,
                        &export,
                        &opts,
                        &NotConfiguredPixelCodec,
                    )
                    .expect("store-identity import");
                }
                "rename" => {
                    mount
                        .connection()
                        .execute(
                            "UPDATE doc_mount_points SET name = ?1 WHERE name = ?2",
                            rusqlite::params![step["to"].as_str(), step["from"].as_str()],
                        )
                        .expect("rename step");
                }
                other => panic!("unknown store-identity step op {other:?}"),
            }
        }
        Ok(())
    })
    .expect("store-identity replay");

    let (stores, docs) = db
        .read_mount_index(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name FROM doc_mount_points WHERE name LIKE 'Identity Store%' \
                 ORDER BY name, id",
            )?;
            let stores: Vec<Value> = stmt
                .query_map([], |r| {
                    Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)? }))
                })?
                .collect::<Result<_, _>>()?;
            let mut stmt = conn.prepare(
                "SELECT p.name, l.mountPointId, l.relativePath \
                   FROM doc_mount_file_links l \
                   JOIN doc_mount_points p ON p.id = l.mountPointId \
                  WHERE p.name LIKE 'Identity Store%' \
                  ORDER BY p.name, p.id, l.relativePath",
            )?;
            let docs: Vec<Value> = stmt
                .query_map([], |r| {
                    Ok(json!({
                        "storeName": r.get::<_, String>(0)?,
                        "storeId": r.get::<_, String>(1)?,
                        "relativePath": r.get::<_, String>(2)?,
                    }))
                })?
                .collect::<Result<_, _>>()?;
            Ok((stores, docs))
        })
        .expect("store-identity dump");
    drop(db);

    let got_v5 = classify_identity_dump(&stores, &docs, &pre_ids);
    let oracle_stores = case["stores"].as_array().cloned().unwrap_or_default();
    let oracle_docs = case["docs"].as_array().cloned().unwrap_or_default();
    let got_v4 = classify_identity_dump(&oracle_stores, &oracle_docs, &pre_ids);

    // The classification must actually key on a PRESERVED archive id — otherwise
    // everything folds to `minted` and the equality below is vacuous. Every arm's
    // ruled end-state lands the archive's documents in a store wearing its own
    // archive id, so at least one store class must be `archive-1`/`archive-2`.
    let has_archive_class = got_v5["stores"]
        .as_array()
        .map(|a| {
            a.iter()
                .any(|s| matches!(s["id"].as_str(), Some("archive-1") | Some("archive-2")))
        })
        .unwrap_or(false);
    if !has_archive_class {
        failures.push(format!(
            "[{name}] no store wears a preserved archive id — either the import did not preserve \
             the archive's id (a v5 regression) or the harness's IDENTITY_ID_* constants drifted \
             from `system-import-execute.test.ts`; the equality below would be vacuous."
        ));
    }

    // Bug 11 convergence: both engines match by id and preserve the archive id,
    // so their classified end-states are EQUAL. If v4 has regressed to matching
    // by name this reddens with the two dumps side by side.
    if got_v5 != got_v4 {
        failures.push(format!(
            "[{name}] the two engines' end-states diverge — bug 11 (store identity is the ID) \
             was expected to have converged\n  rust:   {got_v5}\n  oracle: {got_v4}"
        ));
    }
}

/// The `duplicate`-arm phantom pin: every non-literal chat/project FK a memory
/// row carries must dangle (no chats/projects row has that id) — v4's
/// phantom-map quirk observed end-to-end.
fn assert_phantom_dangles(
    name: &str,
    literals: &HashSet<String>,
    state: &StateDump,
    failures: &mut Vec<String>,
) {
    let empty = Vec::new();
    let ids_of = |table: &str| -> HashSet<String> {
        state
            .get("main")
            .and_then(|t| t.get(table))
            .unwrap_or(&empty)
            .iter()
            .filter_map(|r| r.get("id").and_then(Value::as_str))
            .map(|s| s.to_string())
            .collect()
    };
    let chat_ids = ids_of("chats");
    let project_ids = ids_of("projects");
    let mut phantom_fks = 0usize;
    for row in state
        .get("main")
        .and_then(|t| t.get("memories"))
        .unwrap_or(&empty)
    {
        for (col, targets) in [("chatId", &chat_ids), ("projectId", &project_ids)] {
            if let Some(fk) = row.get(col).and_then(Value::as_str) {
                if !literals.contains(fk) {
                    phantom_fks += 1;
                    if targets.contains(fk) {
                        failures.push(format!(
                            "[{name}] memories.{col} minted FK {fk} RESOLVES to a real row — \
                             v4's phantom duplicate-map quirk is not being reproduced"
                        ));
                    }
                }
            }
        }
    }
    if phantom_fks == 0 {
        failures.push(format!(
            "[{name}] expected at least one phantom memory FK in the duplicate case — the \
             quirk pin never exercised"
        ));
    }
}

#[test]
fn system_import_execute_state_equivalence() {
    let Some(cases) = read_cases() else {
        eprintln!("SKIP: QT_ORACLE_SYSTEM_IMPORT_EXECUTE unset (see the test header).");
        return;
    };

    let user_id = cases
        .iter()
        .find(|l| l["name"] == "_meta")
        .and_then(|m| m["userId"].as_str())
        .expect("oracle carries _meta.userId")
        .to_string();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut failures: Vec<String> = Vec::new();
    let mut ran = 0usize;

    for case in cases.iter().filter(|l| l["name"] != "_meta") {
        let name = case["name"].as_str().unwrap();
        match case["kind"].as_str().unwrap_or("") {
            "execute" => {
                run_execute_case(name, case, &user_id, &mut failures, 1);
                ran += 1;
            }
            // [P4.D46] The embedding-enqueue bail-out arms: a named fixture
            // mutation, then the ordinary execute flow (the prep is applied
            // inside `run_execute_case` when `case.prep` is present).
            "execute_prepped" => {
                run_execute_case(name, case, &user_id, &mut failures, 1);
                ran += 1;
            }
            // [40319484] The only arm that can prove a re-imported archive does
            // not FUSE with its earlier copy: the same payload, twice, into one
            // instance.
            "execute_twice" | "execute_then_rehydrate" => {
                run_execute_case(name, case, &user_id, &mut failures, 2);
                ran += 1;
            }
            // [P4.31] The overwrite-clear's folder gap — see
            // `run_folder_overwrite_case`.
            "execute_folder_overwrite" => {
                run_folder_overwrite_case(name, case, &user_id, &mut failures);
                ran += 1;
            }
            // [P4.33] The store-identity divergence — see
            // `store_identity_expectations`.
            "execute_store_identity" => {
                run_store_identity_case(name, case, &user_id, &mut failures);
                ran += 1;
            }
            "route" => {
                run_route_case(name, case, &user_id, &rt, &mut failures);
                ran += 1;
            }
            other => failures.push(format!("[{name}] unknown oracle kind {other}")),
        }
        if failures.is_empty() {
            eprintln!("OK {name}");
        }
    }

    // 13 from P4.9G4/P4.31 + P4.33's four `store_identity_*` arms + P4.D46's
    // four `execute_files_*` arms and two `execute_prepped` embedding-enqueue
    // bail-out arms + P4.D62's seven `execute_preserve_ids_*` arms + P4.D65's
    // three (the `duplicate` × `preserveIds` corner: the plain fork claiming
    // the carried ids, the same payload under `duplicate` behaving identically
    // because names alone do not conflict, and the id collision that refuses at
    // the preflight — which is WHY the duplicate fork is unreachable there).
    // …+ P4.48's planted preflight-refusal arm + P4.D91's named-item-failure
    // arm.
    // …+ P4.63's bug-105 arm (a plain equality since v4 converged at
    // `679e450e3` — P4.D131).
    // …+ P4.106's three `execute_chats_informs_*` arms (37 + 3 = 40).
    // …+ P4.D226's `execute_concierge_legacy` arm (40 + 1 = 41).
    // …+ P4.130's `execute_concierge_bogus` and `execute_concierge_serde_arm`
    // arms (41 + 2 = 43).
    // …+ P4.143's `execute_duplicate_malformed_profiles` (Tier 2 item 7) and
    // `execute_embedding_provider_enum` (item 8) arms (43 + 2 = 45).
    // …+ P4.148's `execute_project_property_refusals` and
    // `execute_group_property_refusals` arms (45 + 2 = 47).
    // …+ P4.148's `execute_chat_create_db_failure` (C2) arm (47 + 1 = 48).
    // …+ P4.155's two id-less arms (R-E) — `execute_idless_files` and
    // `execute_idless_refused_inserts` (48 + 2 = 50).
    // …+ P4.161's `execute_memory_refusals` and `execute_inform_refusals`
    // (50 + 2 = 52).
    // …+ P4.161 Tier 2's `execute_prompt_template_refusals` (52 + 1 = 53).
    // …+ `execute_folder_refusals` (53 + 1 = 54).
    // …+ `execute_tag_refusals` (54 + 1 = 55).
    // …+ `execute_file_refusals` (55 + 1 = 56).
    assert_eq!(ran, 56, "expected 56 cases, ran {ran}");
    // [P4.161 Tier 2, R-E] Non-vacuity: v4 refused four file rows AFTER their
    // bytes landed (three repository ERRORs each, `skipped` counted) and
    // landed the sound copy; the store contents are the whole-state diff's.
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_file_refusals")
            .expect("the oracle is missing `execute_file_refusals` — regenerate it");
        assert_eq!(
            (
                case["result"]["imported"]["files"].as_i64(),
                case["result"]["skipped"]["files"].as_i64()
            ),
            (Some(1), Some(4)),
            "v4 imports the sound copy and counts the four refusals skipped (R-F)"
        );
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "Data validation failed",
                "Error creating entity",
                "Error creating file"
            ]
            .iter()
            .cycle()
            .take(12)
            .copied()
            .collect::<Vec<_>>(),
            "v4's three repository ERRORs per refused file row"
        );
    }
    // [P4.161 Tier 2] Non-vacuity: v4's tag refusals — two Zod refusals
    // (three lines each), three TypeErrors INSIDE `create` (the wrap alone),
    // one in the importer's duplicate arm (no line) — and three landed (the
    // normalized `quickHide`, the sound tag, the sound duplicate).
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_tag_refusals")
            .expect("the oracle is missing `execute_tag_refusals` — regenerate it");
        assert_eq!(case["result"]["imported"]["tags"].as_i64(), Some(3));
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "Data validation failed",
                "Error creating entity",
                "Error creating tag",
                "Data validation failed",
                "Error creating entity",
                "Error creating tag",
                "Error creating tag",
                "Error creating tag",
                "Error creating tag",
            ],
            "v4's tag refusal lines"
        );
    }
    // [P4.161 Tier 2] Non-vacuity: v4 refused the two bad folders (three
    // repository ERRORs each) and landed the sound one.
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_folder_refusals")
            .expect("the oracle is missing `execute_folder_refusals` — regenerate it");
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "Data validation failed",
                "Error creating entity",
                "Error creating folder"
            ]
            .iter()
            .cycle()
            .take(6)
            .copied()
            .collect::<Vec<_>>(),
            "v4's three repository ERRORs per refused folder"
        );
        let paths: Vec<&str> = case["state"]["main"]["folders"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|f| f["path"].as_str())
            .collect();
        assert!(
            paths.contains(&"/sound-folder")
                && !paths.contains(&"/refused-project")
                && !paths.contains(&"/refused-name"),
            "v4 lands the sound folder only: {paths:?}"
        );
    }
    // [P4.161 Tier 2] Non-vacuity: v4 refused four prompt templates (three
    // repository ERRORs each) and landed the sound twin.
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_prompt_template_refusals")
            .expect("the oracle is missing `execute_prompt_template_refusals` — regenerate it");
        assert_eq!(
            case["result"]["imported"]["promptTemplates"].as_i64(),
            Some(1),
            "v4 lands the sound prompt template only"
        );
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "Data validation failed",
                "Error creating entity",
                "Error creating prompt template"
            ]
            .iter()
            .cycle()
            .take(12)
            .copied()
            .collect::<Vec<_>>(),
            "v4's three repository ERRORs per refused prompt template"
        );
    }
    // [P4.161 — dogfood #152 + R-D] The whole-row refusal arms are non-vacuous
    // only if v4 really refused each bad item (a ZodError tail — or, for the
    // string `tags`, v4's TypeError) and landed the sound ones, with exactly
    // its repository lines per refusal. v5's end-state and lines are the
    // whole-state diff's and `compare_repo_logs`' above.
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_memory_refusals")
            .expect("the oracle is missing `execute_memory_refusals` — regenerate it");
        assert_eq!(
            (
                case["result"]["imported"]["memories"].as_i64(),
                case["result"]["skipped"]["memories"].as_i64(),
            ),
            (Some(2), Some(10)),
            "execute_memory_refusals: v4 lands the sound + defaulted memories and \
             counts the ten refusals `skipped` (R-F)"
        );
        let tails: Vec<&str> = case["result"]["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|w| w.strip_prefix("Failed to import memory: "))
            .collect();
        assert_eq!(tails.len(), 10, "one warning per refused memory: {tails:?}");
        assert_eq!(
            tails.iter().filter(|t| is_zod_error_message(t)).count(),
            9,
            "nine ZodError tails + the string-`tags` TypeError"
        );
        assert!(tails.contains(&"memory.tags.map is not a function"));
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "Data validation failed",
                "Error creating entity",
                "Error creating memory"
            ]
            .iter()
            .cycle()
            .take(27)
            .copied()
            .collect::<Vec<_>>(),
            "execute_memory_refusals: v4's three repository ERRORs per Zod refusal"
        );
        let landed: Vec<&Value> = case["state"]["main"]["memories"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| {
                m["summary"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("Refusal memory "))
            })
            .collect();
        assert_eq!(landed.len(), 2, "v4 lands memories 11 and 12 only");
        let defaulted = landed
            .iter()
            .find(|m| m["summary"] == "Refusal memory 12")
            .expect("the defaulted memory landed");
        assert_eq!(
            (
                defaulted["importance"].as_f64(),
                defaulted["source"].as_str(),
                defaulted["kind"].as_str(),
                defaulted["reinforcementCount"].as_f64(),
                defaulted["reinforcedImportance"].as_f64(),
            ),
            (
                Some(0.5),
                Some("MANUAL"),
                Some("semantic"),
                Some(1.0),
                Some(0.5)
            ),
            "MemorySchema's defaults on the absent keys (R-C)"
        );
    }
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_inform_refusals")
            .expect("the oracle is missing `execute_inform_refusals` — regenerate it");
        assert_eq!(
            case["result"]["imported"]["chatInforms"].as_i64(),
            Some(2),
            "execute_inform_refusals: v4 lands the normalized `permanent: \"yes\"` row and \
             the sound one"
        );
        let tails: Vec<&str> = case["result"]["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|w| w.strip_prefix("Failed to import inform: "))
            .collect();
        assert_eq!(tails.len(), 4, "one warning per refused inform: {tails:?}");
        assert!(tails.iter().all(|t| is_zod_error_message(t)));
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        assert_eq!(
            messages,
            ["Data validation failed", "Error creating entity"]
                .iter()
                .cycle()
                .take(8)
                .copied()
                .collect::<Vec<_>>(),
            "execute_inform_refusals: v4's two lines per refusal and NO WARN (R-D)"
        );
    }
    // [P4.148] The property-refusal arms are non-vacuous only if v4 really
    // refused every bad item with a ZodError tail AND wrote nothing for it,
    // while the sound item landed — so the whole-state equality above is the
    // TRAP's proof (v5 used to half-write each refused project: 6 `projects`
    // rows vs v4's 2, 11 stores vs 7, measured red-first at `07b8f0209`).
    // The projects case ALSO carries a sound item whose two roster keys are
    // explicit `null`s — v4 seeds them before it validates (`prepareCreateData`),
    // so it lands too; the unported P4.148 import refused it (caught at the
    // `07b8f0209` follow-ups unification, red-first on the whole-state diff).
    for (case_name, table, noun, refused, sound) in [
        (
            "execute_project_property_refusals",
            "projects",
            "project",
            9usize,
            &["Project Sound", "Project Null Roster Defaults"][..],
        ),
        (
            "execute_group_property_refusals",
            "groups",
            "group",
            8usize,
            &["Group Sound"][..],
        ),
    ] {
        let case = cases
            .iter()
            .find(|c| c["name"] == case_name)
            .unwrap_or_else(|| panic!("the oracle is missing `{case_name}` — regenerate it"));
        let names: Vec<&str> = case["state"]["main"][table]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r["name"].as_str())
            .collect();
        let cap = format!("{}{}", noun[..1].to_uppercase(), &noun[1..]);
        assert!(
            sound.iter().all(|s| names.contains(s))
                && !names
                    .iter()
                    .any(|n| n.starts_with(&cap) && !sound.contains(n)),
            "{case_name}: v4 must land exactly the sound {noun}(s) {sound:?}: {names:?}"
        );
        let pre_mps = case["preState"]["mountIndex"]["doc_mount_points"]
            .as_array()
            .map_or(0, Vec::len);
        let mps = case["state"]["mountIndex"]["doc_mount_points"]
            .as_array()
            .map_or(0, Vec::len);
        assert_eq!(
            mps,
            pre_mps + sound.len(),
            "{case_name}: one store per sound {noun}"
        );
        let head = format!("Failed to import {noun} \"");
        let tails: Vec<&str> = case["result"]["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|w| w.starts_with(&head))
            .filter_map(|w| w.split_once("\": ").map(|(_, t)| t))
            .collect();
        assert_eq!(tails.len(), refused, "{case_name}: one warning per refusal");
        // [P4.155 R-A] Non-vacuity of the repository-line compare: v4 logged
        // FOUR lines per refusal, in this order — `validate`'s, the
        // store-backed `_create` override's, the store-backed `create`
        // wrap's, then the importer's WARN. (Measured at `94fbb1ae3`: the
        // order's "the base pair through `log_create_failure`" was wrong —
        // the store-backed base overrides `createErrorMessage()`.)
        let messages: Vec<&str> = case["repoLogs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["message"].as_str())
            .collect();
        let per_refusal = [
            "Data validation failed".to_string(),
            format!("Error creating {noun} entity"),
            format!("Error creating {noun}"),
            format!("Failed to import {noun}"),
        ];
        assert_eq!(
            messages,
            per_refusal
                .iter()
                .cycle()
                .take(4 * refused)
                .map(String::as_str)
                .collect::<Vec<_>>(),
            "{case_name}: v4's repository lines per refusal"
        );
        assert!(
            tails.iter().all(|t| is_zod_error_message(t)),
            "{case_name}: every tail is a ZodError message: {tails:?}"
        );
    }
    // [P4.130] The refused-chat arm is non-vacuous only if v4 really refused
    // the bogus chat with its ZodError bytes AND landed the neighbour: the
    // bogus chat is ABSENT from v4's end-state, the neighbour PRESENT, and
    // v4's result carries exactly one `Failed to import chat` warning whose
    // tail survives the mask verbatim (the mask exception above). v5's
    // end-state and masked result are the whole-state diff's, so they agree.
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_concierge_bogus")
            .expect("the oracle is missing `execute_concierge_bogus`");
        let titles: Vec<&str> = case["state"]["main"]["chats"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["title"].as_str())
            .filter(|t| t.starts_with("Concierge Bogus "))
            .collect();
        assert_eq!(
            titles,
            vec!["Concierge Bogus 7"],
            "v4 must skip the bogus chat and land its neighbour"
        );
        let warned: Vec<String> = case["result"]["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|w| w.starts_with("Failed to import chat \"Concierge Bogus 6\": "))
            .map(mask_warning)
            .collect();
        assert_eq!(warned.len(), 1, "v4's refusal warning: {warned:?}");
        assert!(
            warned[0].contains("\"code\": \"invalid_value\"")
                && warned[0].contains("\"conciergeMode\"")
                && !warned[0].contains("<ENGINE>"),
            "the ZodError tail must survive the mask verbatim: {}",
            warned[0]
        );
    }
    // [P4.D226] The legacy-Concierge arm is non-vacuous only if v4's end-state
    // really took every row of the derive table — a payload that lost its
    // legacy keys would land five Moderated chats on BOTH sides and still be
    // equal. v5's end-state is the whole-state diff's.
    {
        let case = cases
            .iter()
            .find(|c| c["name"] == "execute_concierge_legacy")
            .expect("the oracle is missing `execute_concierge_legacy`");
        let mut got: Vec<(String, Value, Value, Value)> = case["state"]["main"]["chats"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                let title = c["title"].as_str()?;
                title.starts_with("Concierge Legacy ").then(|| {
                    (
                        title.to_string(),
                        c["conciergeMode"].clone(),
                        c["conciergeModeSetBy"].clone(),
                        c["conciergeModeReason"].clone(),
                    )
                })
            })
            .collect();
        got.sort_by(|a, b| a.0.cmp(&b.0));
        let row = |n: u8, m: &str, by: Value, why: Value| {
            (format!("Concierge Legacy {n}"), json!(m), by, why)
        };
        assert_eq!(
            got,
            vec![
                row(1, "unmoderated", json!("operator"), json!("migration")),
                row(2, "locked", json!("operator"), json!("migration")),
                row(3, "unmoderated", json!("concierge"), json!("classifier")),
                row(4, "locked", json!("operator"), json!("manual")),
                row(5, "moderated", Value::Null, Value::Null),
            ],
            "v4's end-state must carry every row of the legacy derive table"
        );
    }
    // [P4.106 item 6] The chats-only `.qtap` over planted informs — non-vacuous
    // by construction: the payload carries all five rows (two pending, two
    // consumed, one for a seat its chat lacks), the target starts with none,
    // and the cross-instance arm's v4 end-state lands the four seated rows (the
    // fifth dropped by name). v5's end-states are the whole-state diff's.
    for arm in [
        "execute_chats_informs_skip",
        "execute_chats_informs_duplicate",
        "execute_chats_informs_cross_instance",
    ] {
        let case = cases
            .iter()
            .find(|c| c["name"] == arm)
            .unwrap_or_else(|| panic!("the oracle is missing `{arm}`"));
        let carried = case["exportData"]["data"]["chatInforms"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        assert_eq!(carried, 5, "[{arm}] the payload's chatInforms");
        let pre = case["preState"]["main"]["chat_informs"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        assert_eq!(pre, 0, "[{arm}] the target starts with no informs");
    }
    let cross = cases
        .iter()
        .find(|c| c["name"] == "execute_chats_informs_cross_instance")
        .unwrap();
    assert_eq!(
        cross["state"]["main"]["chat_informs"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
        4,
        "the cross-instance import must land the four seated informs (v4's end-state; v5's \
         is the whole-state diff above)"
    );
    // [P4.63 → bug 105] Named explicitly, so a dropped case fails saying WHAT
    // went missing rather than failing the arithmetic above.
    assert!(
        cases
            .iter()
            .any(|c| c["name"] == "execute_bug105_seed_abort"),
        "the oracle is missing the `execute_bug105_seed_abort` regression-guard \
         case — regenerate it"
    );
    // [P4.D91 → bug 79] The five per-item arms v4 `275cd7bc` gave named
    // warnings must be MEASURED, not merely ported: no committed archive
    // contains an item that fails to import, so the corpus carries a payload
    // built to fail one item per arm. Assert the case is present AND that it
    // still names all five — a payload edit that made an item importable would
    // otherwise leave the whole family silently unmeasured.
    // [P4.70] The connection-profile leg measured NOTHING from v4 bug 68 until
    // the `system-data-*` fixture was widened to the baseline vintage: both
    // engines threw on every import, and the masked sentences agreed. This
    // tripwire keeps the closure self-guarding — a fixture that regresses in
    // vintage puts the count back to 0 on BOTH sides and this fails by name
    // instead of going green-and-vacuous (the §3 unification review).
    for arm in ["execute_overwrite_all", "execute_duplicate_all"] {
        let row = cases
            .iter()
            .find(|c| c["name"] == arm)
            .unwrap_or_else(|| panic!("the oracle is missing `{arm}` — regenerate it"));
        let imported = row["result"]["imported"]["connectionProfiles"].as_i64();
        assert!(
            imported.is_some_and(|n| n >= 1),
            "`{arm}` imported {imported:?} connection profiles — the leg has gone \
             vacuous again (fixture vintage? see the header)"
        );
    }
    let named = cases
        .iter()
        .find(|c| c["name"] == "execute_named_item_failures")
        .expect("the oracle is missing `execute_named_item_failures` — regenerate it");
    for family in [
        "Failed to import tag \"Broken Tag\": ",
        "Failed to import connection profile \"Broken Connection\": ",
        "Failed to import image profile \"Broken Image\": ",
        "Failed to import embedding profile \"Broken Embedding\": ",
        "Failed to import roleplay template \"Broken Template\": ",
    ] {
        assert!(
            named["result"]["warnings"]
                .as_array()
                .map(|a| a
                    .iter()
                    .any(|w| w.as_str().is_some_and(|s| s.starts_with(family))))
                .unwrap_or(false),
            "v4 no longer names the `{family}` failure — the arm has stopped \
             measuring what it exists for.\n  oracle: {}",
            named["result"]["warnings"]
        );
    }
    // …and the preserveIds family asserted by SHAPE, not just by the total, so a
    // truncated oracle cannot pass by arithmetic (the corpus-shape lesson). Each
    // arm is the only one covering its behaviour: the two refusal SENTENCES, the
    // land-then-rehydrate round trip, the fixture's own repeat-shaped bundle, the
    // foreign-collision refusal, and Bug 54's two sha-first outcomes.
    for arm in [
        "execute_preserve_ids_refuse_existing",
        "execute_preserve_ids_repeat_in_bundle",
        "execute_preserve_ids_vault",
        "execute_preserve_ids_skip_if_present",
        "execute_preserve_ids_skip_foreign_refuses",
        "execute_preserve_ids_dedup_by_sha",
        "execute_preserve_ids_sha_mismatch_refuses",
        "execute_preserve_ids_plain_claims_ids",
        "execute_preserve_ids_duplicate_free_ids",
        "execute_preserve_ids_duplicate_existing_id_refuses",
        // [P4.48] the refusal when the existence read FAILS rather than misses
        "execute_preserve_ids_unavailable_store_refuses",
        // [P4.D91 → bug 79] the refusal when the destination row is READABLE
        // but unvalidatable — the one plant that reaches v4's strict scope
        "execute_preserve_ids_unvalidatable_row_refuses",
    ] {
        assert!(
            cases.iter().any(|c| c["name"] == arm),
            "the oracle is missing the `{arm}` preserveIds arm — regenerate it"
        );
    }
    assert!(
        V5_TRANSCRIPT_MAX.load(Ordering::Relaxed) > 0,
        "v5 left every chat's `transcriptVersion` at zero across the whole \
         corpus, so the column's agreement with v4 above is vacuous. Either \
         the import path stopped reaching the message funnel or the funnel \
         stopped bumping — see `record_v5_transcript_counters`."
    );
    assert!(
        failures.is_empty(),
        "{} import-state difference(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // [P4.143] Every serde-arm row was classified — a row whose case never ran
    // is a failure, not a skip (a row whose HEAD is missing already reported
    // WRONG SHAPE by name).
    assert_eq!(
        SERDE_ARMS_EXERCISED.load(Ordering::SeqCst),
        SERDE_ARM_DIVERGENCES.len(),
        "SERDE_ARM_DIVERGENCES rows exercised"
    );
    // [P4.143 Tier 2 item 10] Both chat refusal arms compared their
    // repository lines (`execute_concierge_bogus` byte for byte,
    // `execute_concierge_serde_arm` through its table row) — and, since
    // P4.148, the DB-error arm (`execute_chat_create_db_failure`: a SQLite
    // throw AFTER validation — `Error creating entity` + `Failed to create
    // chat` with `strictFailures`, NO `Data validation failed`; C2).
    // [P4.148 Tier 2 item 17] The nine import WARNs compared on every execute
    // case (35 at `07b8f0209`); v4 fires two lines across them —
    // `execute_files_cross_instance`'s `Failed to import file` and
    // `execute_memories_no_profile`'s `Imported memories left unembedded`; the
    // other cases are silence legs. (The other seven lines fire on no oracle
    // case; their unit pins live beside them in `services::quilltap_import`.)
    // [P4.155 R-E] …+ the two id-less arms (37): `execute_idless_files` fires
    // one `Failed to import file` with NO `fileId`, and
    // `execute_idless_refused_inserts` three `Failed to import memory` (one
    // id-less) and two `Failed to import prompt template` (one id-less) —
    // 2 + 1 + 5 = 8 lines.
    // [P4.161] …+ the two refusal arms (39): `execute_memory_refusals` fires
    // ten `Failed to import memory` (one id-less — the C6 walk shape), the
    // inform arm none (R-D) — 8 + 10 = 18 lines.
    // [P4.161 Tier 2] …+ the prompt-template arm (40), four more lines (22).
    assert_eq!(
        IMPORT_WARN_CASES.load(Ordering::SeqCst),
        43,
        "cases comparing the import WARNs"
    );
    assert_eq!(
        IMPORT_WARNS_FIRED.load(Ordering::SeqCst),
        28,
        "v4 import WARN lines fired"
    );
    // [P4.155 R-A] …+ the two property-refusal arms (5).
    // [P4.161] …+ the memory and inform refusal arms (7), + Tier 2's
    // prompt-template arm (8).
    assert_eq!(
        REPO_LOG_CASES.load(Ordering::SeqCst),
        11,
        "cases that compared a refused create's repository lines"
    );
    assert_eq!(
        SERDE_ARM_DIVERGENCES.len(),
        8,
        "the chat row + the five remaining create rows (P4.161 retired the tag row) + \
         P4.143 Tier 2's two duplicate-arm rows"
    );
}

/// [P4.31 → P4.33 → bug 11] The `.qtap` overwrite-clear's FOLDER behavior —
/// the semantics half, now a PLAIN equality.
///
/// Two imports into one store: the first seeds `alpha` + `alpha/beta`, the
/// second overwrites it with an archive carrying only `gamma`. v4's overwrite
/// branch used to clear documents, blobs, chunks and files but NOT
/// `doc_mount_folders`, leaving all THREE folders — stale husks, the orphan
/// shape P4.31 closed at the delete end.
///
/// P4.31 measured this and escalated rather than guessing, because clearing the
/// table also takes the scaffolding a vault / project store is provisioned with
/// (`Outfits` / `Prompts` / `Scenarios` / `Wardrobe` / `files` / `images`).
/// **The ruling (human, 2026-08-04) settled it: overwrite means overwrite.** A
/// real export always carries the scaffolding back — v4's own exporter dumps
/// every folder row (`lib/export/ndjson-writer.ts:513-524`) — so the
/// scaffold-loss arm is an archive shape no real export produces, and
/// round-trip fidelity wins. v5 made this fix first; v4 has since CONVERGED
/// (`3bb664f0`, bug 11: `import-document-stores.ts` clears folders too).
///
/// So both sides now end with EXACTLY the second archive's folders — asserted on
/// each engine (a regression on either side reddens with the paths side by side).
///
/// Everything else on this case is still compared for equality — both result
/// bodies, the store count, and each link with the PATH of the folder it
/// resolves to. That last one is the fidelity claim worth having: v5's `gamma`
/// link must resolve to v5's freshly written `gamma` row, not dangle.
///
/// It does not reuse the `execute_twice` path because that comparison runs the
/// whole three-partition state through a normalizer that labels minted ids in
/// walk order; a divergence in row COUNT shifts every label after it. The
/// oracle emits a focused, id-free dump instead (folder paths, link paths with
/// the folder each resolves to, the store count) plus both result bodies.
fn run_folder_overwrite_case(name: &str, case: &Value, user_id: &str, failures: &mut Vec<String>) {
    let scratch = fresh_fixture(name);
    let first = export_of(&case["firstData"]);
    let second = export_of(&case["exportData"]);
    let opts = options_of(&case["options"]);
    let uid = user_id.to_string();

    let db = open_db(&scratch);
    let results = db
        .write_blocking(move |ws| {
            let main = ws.main().connection();
            let mount = ws.mount_index().expect("fixture has a mount partition");
            let r1 = execute_import(
                main,
                mount.connection(),
                &uid,
                &first,
                &opts,
                &NotConfiguredPixelCodec,
            )
            .expect("first import");
            let r2 = execute_import(
                main,
                mount.connection(),
                &uid,
                &second,
                &opts,
                &NotConfiguredPixelCodec,
            )
            .expect("second import");
            Ok(vec![r1.to_value(), r2.to_value()])
        })
        .expect("imports ran");

    let (folders, links, store_count) = db
        .read_mount_index(|conn| {
            let store: Option<String> = conn
                .query_row(
                    "SELECT id FROM doc_mount_points WHERE name = 'Folder Store'",
                    [],
                    |r| r.get(0),
                )
                .ok();
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM doc_mount_points WHERE name = 'Folder Store'",
                [],
                |r| r.get(0),
            )?;
            let Some(store) = store else {
                return Ok((Value::Array(vec![]), Value::Array(vec![]), count));
            };
            let mut stmt = conn.prepare(
                "SELECT path, name FROM doc_mount_folders WHERE mountPointId = ?1 \
                 ORDER BY path, name",
            )?;
            let folders: Vec<Value> = stmt
                .query_map([&store], |r| {
                    Ok(json!({ "path": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)? }))
                })?
                .collect::<Result<_, _>>()?;
            let mut stmt = conn.prepare(
                "SELECT l.relativePath, \
                        CASE WHEN l.folderId IS NULL THEN NULL \
                             ELSE COALESCE(f.path, '<dangling>') END \
                   FROM doc_mount_file_links l \
                   LEFT JOIN doc_mount_folders f ON f.id = l.folderId \
                  WHERE l.mountPointId = ?1 ORDER BY l.relativePath",
            )?;
            let links: Vec<Value> = stmt
                .query_map([&store], |r| {
                    Ok(json!({
                        "relativePath": r.get::<_, String>(0)?,
                        "folderPath": r.get::<_, Option<String>>(1)?,
                    }))
                })?
                .collect::<Result<_, _>>()?;
            Ok((Value::Array(folders), Value::Array(links), count))
        })
        .expect("folder-overwrite dump");
    drop(db);

    let mut bad = |msg: String| failures.push(format!("[{name}] {msg}"));

    // Everything but the folder rows must be identical, including both bodies.
    for (i, want_key) in ["result", "result2"].iter().enumerate() {
        if results[i] != case[*want_key] {
            bad(format!(
                "{want_key} diverged:\n  rust  : {}\n  oracle: {}",
                results[i], case[*want_key]
            ));
        }
    }
    if links != case["links"] {
        bad(format!(
            "link/folder resolution diverged:\n  rust  : {links}\n  oracle: {}",
            case["links"]
        ));
    }
    if json!(store_count) != case["storeCount"] {
        bad(format!(
            "store count {store_count} != oracle {}",
            case["storeCount"]
        ));
    }

    // ── the overwrite-clear folders (a PLAIN equality since bug 11 converged) ──
    //
    // The archive's own folder is `gamma`; an overwrite that means overwrite
    // leaves exactly the archive's tree, with no husks. v5 made this fix first;
    // v4 has adopted it (`import-document-stores.ts` clears folders too), so both
    // sides must now end with EXACTLY the archive's folders.
    let paths = |v: &Value| -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|r| r["path"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let archive_paths = second_payload_folder_paths(&case["exportData"]);

    let mut v5_paths = paths(&folders);
    v5_paths.sort();
    if v5_paths != archive_paths {
        bad(format!(
            "v5 must end with EXACTLY the archive's folders {archive_paths:?}, got {v5_paths:?} \
             — the ruled overwrite-clear (see `overwrite_clear_mount`) is not holding."
        ));
    }

    let mut v4_paths = paths(&case["folders"]);
    v4_paths.sort();
    if v4_paths != archive_paths {
        bad(format!(
            "v4 must end with EXACTLY the archive's folders {archive_paths:?}, got {v4_paths:?} \
             — if v4 has REGRESSED to keeping stale folder husks, restore the both-directions \
             divergence pin (see the git history for bug 11's FOLDER_CLEAR_DIVERGENCE)."
        ));
    }
}

/// The folder paths the SECOND payload actually carries — read from the oracle's
/// own emitted `exportData` rather than hard-coded, so a payload edit cannot
/// leave this arm asserting a stale expectation.
fn second_payload_folder_paths(export_data: &Value) -> Vec<String> {
    let mut out: Vec<String> = export_data["data"]["folders"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f["path"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// [P4.D91 → bug 79] The two engines refuse the same import for different
/// reasons, because they fail at different depths.
///
/// The destination's `tags` row has a `createdAt` its schema rejects. v4 cannot
/// read the row at all — `_findById` validates, the validation throws, and
/// (since `275cd7bc` wrapped the import in a strict scope) the throw reaches the
/// preflight's catch instead of degrading to `null`. So v4 refuses naming the
/// VALIDATION failure, and never learns that the id is taken.
///
/// v5 marshals rows without re-validating them, so it reads the row, sees the
/// claimed id is taken, and refuses naming the COLLISION — the more useful
/// sentence, and the one v4 itself emits for every readable collision.
///
/// Both refuse before a single write, which is bug 79's whole guarantee and is
/// asserted as a plain equality on the state dumps. Only the sentences differ,
/// so they are asserted here and then replaced with one placeholder on both
/// sides. Two-directional: if v4 stops refusing (its swallow returning), or v5
/// stops naming the collision, this fires.
///
/// What this arm does NOT prove: v5's own P4.48 propagation. The plant is a
/// read failure on the v4 side only — v5 does not re-validate rows on read, so
/// it never sees an error to propagate here. Reverting this preflight site to
/// `.ok().flatten()` leaves the arm green, by design; the sites that DO
/// exercise it are `execute_preserve_ids_unavailable_store_refuses` and the
/// preview family's planted arms.
fn classify_unvalidatable_row(
    name: &str,
    got_body: &Value,
    want_body: &Value,
    failures: &mut Vec<String>,
) -> (Value, Value) {
    let warnings_of = |b: &Value| -> Vec<String> {
        b["warnings"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|w| w.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    const REFUSED: &str = "Import refused before anything was written: ";
    const COLLISION: &str = "Preserve IDs collision for tag a5000000-0000-4000-8000-000000000002";

    let v4 = warnings_of(want_body);
    let v4_ok = v4.len() == 1
        && v4[0].starts_with(REFUSED)
        && !v4[0].starts_with(&format!("{REFUSED}Preserve IDs collision"));
    if !v4_ok {
        failures.push(format!(
            "[{name}] v4 no longer refuses on the unreadable destination row with a \
             single non-collision refusal — either the bug-79 strict scope stopped \
             propagating (v4 is swallowing again) or it learned to read the row. \
             Re-measure.\n  oracle warnings: {v4:?}"
        ));
    }

    let v5 = warnings_of(got_body);
    let v5_ok = v5.len() == 2 && v5[0] == COLLISION && v5[1] == format!("{REFUSED}{COLLISION}");
    if !v5_ok {
        failures.push(format!(
            "[{name}] v5 must refuse naming the COLLISION (it can read the row) and \
             wrap it with the refusal line.\n  rust warnings: {v5:?}"
        ));
    }

    let blank = |b: &Value| {
        let mut out = b.clone();
        out["warnings"] = Value::Array(vec![Value::String(
            "<REFUSAL SENTENCES: recorded divergence, asserted above>".to_string(),
        )]);
        out
    };
    (blank(got_body), blank(want_body))
}

fn run_execute_case(
    name: &str,
    case: &Value,
    user_id: &str,
    failures: &mut Vec<String>,
    runs: usize,
) {
    let scratch = fresh_fixture(name);

    // [P4.D46] `execute_prepped`: apply the case's NAMED fixture mutation
    // before the baseline dump, mirroring the oracle's prep (which also runs
    // before ITS preState dump — so the baselines still compare byte-equal).
    if let Some(prep) = case.get("prep").and_then(Value::as_str) {
        let conn =
            quilltap_core::db::Writer::open_writable(&scratch.root.join("main.db"), TEST_PEPPER)
                .expect("open main for prep");
        match prep {
            "drop-embedding-profiles" => conn
                .connection()
                .execute_batch("DELETE FROM \"embedding_profiles\"")
                .expect("prep: drop embedding profiles"),
            "builtin-default-profile" => conn
                .connection()
                .execute_batch("UPDATE \"embedding_profiles\" SET \"provider\" = 'BUILTIN'")
                .expect("prep: builtin default profile"),
            // [P4.48] Orphan PROJECT_1's document store: the slim row survives,
            // so the preflight's existence read SUCCEEDS and the overlay throws
            // — the one leg where v4 propagates too.
            "orphan-project-store" => {
                let touched = conn
                    .connection()
                    .execute(
                        "UPDATE projects SET officialMountPointId = NULL WHERE id = ?1",
                        ["a3000000-0000-4000-8000-000000000001"],
                    )
                    .expect("prep: orphan project store");
                // A plant that touches nothing leaves a vacuously green arm.
                assert_eq!(
                    touched, 1,
                    "[{name}] the fixture no longer carries PROJECT_1 — the prep \
                     would be a no-op and the arm vacuous"
                );
            }
            // [P4.D91] A row SQLite hands back happily and v4's schema
            // refuses (`createdAt` is `z.iso.datetime()`). Unlike a dropped
            // table — which v4's `ensureCollection` silently rebuilds — nothing
            // heals this, so it is the one plant that reaches bug 79's strict
            // scope on the v4 side.
            "unvalidatable-tag" => {
                let touched = conn
                    .connection()
                    .execute(
                        "UPDATE tags SET \"createdAt\" = 'not-a-date' WHERE id = ?1",
                        ["a5000000-0000-4000-8000-000000000002"],
                    )
                    .expect("prep: unvalidatable tag");
                assert_eq!(
                    touched, 1,
                    "[{name}] the fixture no longer carries TAG_2 — the prep \
                     would be a no-op and the arm vacuous"
                );
            }
            // [P4.148] Every `chats` INSERT refused by SQLite AFTER the row
            // validated — the import's chat-create DB-error arm (C2).
            "refuse-chat-inserts" => conn
                .connection()
                .execute_batch(
                    "CREATE TRIGGER qt_p4148_no_chats BEFORE INSERT ON chats BEGIN \
                     SELECT RAISE(ABORT, 'planted: chat inserts refused'); END",
                )
                .expect("prep: refuse chat inserts"),
            // [P4.155 R-E] Every `memories` / `prompt_templates` INSERT refused,
            // so each payload item — one of each kind WITH an id, one WITHOUT
            // — lands in its importer's per-item WARN.
            "refuse-idless-inserts" => conn
                .connection()
                .execute_batch(
                    "CREATE TRIGGER qt_p4155_no_memories BEFORE INSERT ON memories BEGIN \
                     SELECT RAISE(ABORT, 'planted: memory inserts refused'); END; \
                     CREATE TRIGGER qt_p4155_no_templates BEFORE INSERT ON prompt_templates \
                     BEGIN SELECT RAISE(ABORT, 'planted: template inserts refused'); END",
                )
                .expect("prep: refuse id-less inserts"),
            other => panic!("[{name}] unknown prep {other}"),
        }
    }

    // The PRE-import baseline: both sides copy the same fixture bytes, so the
    // raw dumps must be EQUAL — anything else is dump-machinery drift, reported
    // as such rather than leaking into every table diff below.
    let pre = read_state(&scratch);
    let want_pre = state_from_value(&case["preState"]);
    // [P4.106] Return early ONLY when `diff_states` itself recorded a baseline
    // difference. The guard used to be a strict `pre != want_pre`, but
    // `diff_states` reads an ABSENT table as empty — so a baseline that differed
    // only by an empty table on one side (v5's copy carries the booted
    // `chat_informs`, v4's lazily-created collection does not) pushed NO
    // failure and still returned, skipping the whole case silently while the
    // loop printed `OK`. Measured: the three `execute_chats_informs_*` arms
    // "passed" with v5's inform import disabled outright.
    let baseline_failures = failures.len();
    diff_states(&format!("{name} BASELINE"), &pre, &want_pre, failures);
    if failures.len() > baseline_failures {
        return;
    }

    let export = export_of(&case["exportData"]);
    // [P4.D62] `execute_then_rehydrate` runs the SAME payload twice under
    // DIFFERENT options: run 1 lands it (refuse-on-collision), run 2 replays it
    // as a rehydrate (skip-if-present). Every other multi-run kind repeats one
    // bag, which `options2` absent reproduces.
    let opts = options_of(&case["options"]);
    let opts2 = match case.get("options2") {
        Some(o) if !o.is_null() => options_of(o),
        _ => opts.clone(),
    };
    let uid = user_id.to_string();

    // [P4.143 Tier 2 item 10] A case whose oracle recorded the refused chat
    // create's repository lines is CAPTURED — inside the write closure, which
    // runs on the WRITER thread (a thread-scoped subscriber on the test thread
    // would see nothing); `execute_import` is synchronous there.
    let capture_repo_logs = case.get("repoLogs").is_some();
    // [P4.148 Tier 2 item 17] Every execute case records v4's nine
    // `IMPORT_WARN_MESSAGES` lines — capture v5's for the same compare.
    let capture_import_warns = case.get("importWarns").is_some();
    let capture = capture_repo_logs || capture_import_warns;
    let db = open_db(&scratch);
    let (results, repo_lines) = db
        .write_blocking(move |ws| {
            let main = ws.main().connection();
            let mount = ws.mount_index().expect("fixture has a mount partition");
            let run_all = || -> Result<Vec<_>, quilltap_core::db::DbError> {
                let mut out = Vec::new();
                for i in 0..runs {
                    let opts = if i == 0 { &opts } else { &opts2 };
                    out.push(
                        execute_import(
                            main,
                            mount.connection(),
                            &uid,
                            &export,
                            opts,
                            &NotConfiguredPixelCodec,
                        )
                        .map_err(|e| match e {
                            quilltap_core::services::quilltap_import::ImportError::Db(d) => d,
                            other => {
                                panic!("unexpected parse-side error from execute: {other}")
                            }
                        })?,
                    );
                }
                Ok(out)
            };
            if capture {
                let (out, lines) = quilltap_core::test_support::captured_with(run_all);
                Ok((out?, lines))
            } else {
                Ok((run_all()?, Vec::new()))
            }
        })
        .expect("execute_import ran");
    if capture_repo_logs {
        // The chat serde arm's row of `SERDE_ARM_DIVERGENCES` carves the
        // `error` field too — the SAME table as the warning.
        let serde = SERDE_ARM_DIVERGENCES
            .iter()
            .find(|a| a.case == name && a.head.starts_with("Failed to import chat \""))
            .map(|a| (a.v4_path_key, a.v5_serde_prefix));
        compare_repo_logs(
            name,
            &case["repoLogs"],
            &repo_lines,
            serde,
            // `executeImport` runs inside `withStrictRepositoryFailures`
            // (`execute.ts:425-431`): every `safeQuery`-born line carries it —
            // the chat create's two, and the refused project / group create's
            // `_create` + store-backed `create` pair (P4.155, R-A).
            &[
                "Error creating entity",
                "Failed to create chat",
                "Error creating project entity",
                "Error creating group entity",
                "Error creating project",
                "Error creating group",
                // [P4.161] the refused memory's wrap (its `Data validation
                // failed` is a DIRECT logger call — never strict, measured).
                "Error creating memory",
                "Error creating prompt template",
                "Error creating folder",
                "Error creating tag",
                "Error creating file",
            ],
            failures,
        );
        REPO_LOG_CASES.fetch_add(1, Ordering::SeqCst);
    }
    if capture_import_warns {
        let want: Vec<String> = case["importWarns"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|w| w.as_str().map(str::to_string))
            .collect();
        let got = v5_import_warns(&repo_lines);
        if got != want {
            failures.push(format!(
                "[{name}] the nine import WARN lines differ\n  rust:   {got:?}\n  oracle: {want:?}"
            ));
        }
        IMPORT_WARN_CASES.fetch_add(1, Ordering::SeqCst);
        IMPORT_WARNS_FIRED.fetch_add(want.len(), Ordering::SeqCst);
    }
    drop(db);

    let got_state = read_state(&scratch);
    let want_state = state_from_value(&case["state"]);
    let literals = literals_for(&pre, &case["exportData"]);

    // [P4.33 → bug 11] The folder-clear and store-create divergences RETIRED:
    // v4 converged, so every whole-state arm is a plain equality (no folder or
    // store labelling, no comparand subtraction).
    let (got_body, want_body) = (results[0].to_value(), case["result"].clone());
    let got_labels: BTreeMap<String, String> = BTreeMap::new();
    let want_labels: BTreeMap<String, String> = BTreeMap::new();

    // ⚠ bug 10's per-chat annotation sweep is P4.D53's — carve it out until then.

    // [P4.D91 → bug 79] The unvalidatable-row arm's warnings are a RECORDED
    // DIVERGENCE, pinned in both directions; everything else about the case —
    // `success`, the counts, all three partitions — is a plain equality, which
    // is the claim that matters: neither engine writes anything.
    //
    // [P4.63 → bug 105 → P4.D131] The seeding-helper abort was the second such
    // arm — and the one place a table was SUBTRACTED from the comparands. v4
    // converged at `679e450e3`, so `execute_bug105_seed_abort` is now an
    // ordinary plain-equality case and the table-skip machinery it carried is
    // gone with it (see the module header).
    let (got_body, want_body) = if name == "execute_preserve_ids_unvalidatable_row_refuses" {
        classify_unvalidatable_row(name, &got_body, &want_body, failures)
    } else if SERDE_ARM_DIVERGENCES.iter().any(|a| a.case == name) {
        classify_serde_arms(name, &got_body, &want_body, failures)
    } else {
        (got_body, want_body)
    };

    compare_execute(
        name,
        &got_body,
        &want_body,
        &got_state,
        &want_state,
        &literals,
        got_labels.clone(),
        want_labels.clone(),
        failures,
    );
    if runs > 1 {
        // The second run's body is compared too — a fused re-import would show
        // up in its counts long before the state diff.
        let (got2, want2) = (results[1].to_value(), case["result2"].clone());
        let literals2 = literals.clone();
        let (got_norm, _) = normalize_side(
            &literals2,
            derived_hashes(&got_state, &literals2),
            &got2,
            &got_state,
            got_labels,
        );
        let (want_norm, _) = normalize_side(
            &literals2,
            derived_hashes(&want_state, &literals2),
            &want2,
            &want_state,
            want_labels,
        );
        if got_norm != want_norm {
            failures.push(format!(
                "[{name}] SECOND import result body differs\n  rust:   {got_norm}\n  oracle: {want_norm}"
            ));
        }
    }

    if name == "execute_duplicate_all" {
        assert_phantom_dangles(name, &literals, &got_state, failures);
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_execute(
    name: &str,
    got_result: &Value,
    want_result: &Value,
    got_state: &StateDump,
    want_state: &StateDump,
    literals: &HashSet<String>,
    got_labels: BTreeMap<String, String>,
    want_labels: BTreeMap<String, String>,
    failures: &mut Vec<String>,
) {
    // P4.6BK closed the chunk gap, so nothing is skipped: every table,
    // including `doc_mount_chunks`, is diffed row for row. (The bug-105
    // divergence arm briefly subtracted `main.image_profiles` here — retired
    // at P4.D131 when v4 converged at `679e450e3`.)
    let (got_norm_result, got_norm_state) = normalize_side(
        literals,
        derived_hashes(got_state, literals),
        got_result,
        got_state,
        got_labels,
    );
    let (want_norm_result, want_norm_state) = normalize_side(
        literals,
        derived_hashes(want_state, literals),
        want_result,
        want_state,
        want_labels,
    );

    if got_norm_result != want_norm_result {
        failures.push(format!(
            "[{name}] result body differs\n  rust:   {got_norm_result}\n  oracle: {want_norm_result}"
        ));
    }
    record_v5_transcript_counters(&got_norm_state);
    diff_states(name, &got_norm_state, &want_norm_state, failures);
}

fn run_route_case(
    name: &str,
    case: &Value,
    user_id: &str,
    rt: &tokio::runtime::Runtime,
    failures: &mut Vec<String>,
) {
    let status = case["status"].as_u64().unwrap_or(0);
    let body = &case["body"];

    // The multipart options-part arm is web-edge-only (quilltap-web's
    // `qtap_routes::import_execute`); its v5 string is pinned by that crate's
    // unit test against this same literal. Here we assert v4's side of the pin.
    if name == "route_multipart_bad_options" {
        if status != 400 || body["error"] != json!("Invalid JSON: Failed to parse options") {
            failures.push(format!(
                "[{name}] v4's multipart bad-options arm moved: status {status}, body {body}"
            ));
        }
        return;
    }

    let scratch = fresh_fixture(name);
    let pre = read_state(&scratch);
    let db = open_db(&scratch);

    let request_body = &case["requestBody"];
    let export_data = request_body
        .get("exportData")
        .cloned()
        .unwrap_or(Value::Null);
    let options = request_body.get("options").cloned().unwrap_or(Value::Null);

    let resp = rt.block_on(system_qtap::import_execute(
        &db,
        user_id,
        &export_data,
        &options,
        None,
    ));
    drop(db);

    match resp {
        Response::Error(e) => {
            let want_msg = body.get("error").and_then(Value::as_str).unwrap_or("");
            let kind_ok = matches!(e.kind, ErrorKind::BadRequest) && status == 400;
            if !kind_ok || e.message != want_msg {
                failures.push(format!(
                    "[{name}] error arm differs: rust ({:?}, {:?}) vs oracle ({status}, {want_msg:?})",
                    e.kind, e.message
                ));
            }
            // A validation refusal must write NOTHING.
            let post = read_state(&scratch);
            if post != pre {
                failures.push(format!(
                    "[{name}] a validation-failure arm MUTATED the database"
                ));
            }
        }
        Response::System(got_body) => {
            if status != 200 {
                failures.push(format!(
                    "[{name}] rust answered a body where the oracle answered status {status}"
                ));
                return;
            }
            let literals = literals_for(&pre, &export_data);
            if case.get("state").filter(|v| !v.is_null()).is_some() {
                let got_state = read_state(&scratch);
                let want_state = state_from_value(&case["state"]);
                // [P4.33 → bug 11] The folder-clear divergence RETIRED — v4
                // converged, so `route_replace_remap` is a plain equality.
                let (got_body, want_body) = (got_body.clone(), body.clone());
                let got_labels: BTreeMap<String, String> = BTreeMap::new();
                let want_labels: BTreeMap<String, String> = BTreeMap::new();
                // ⚠ bug 10's per-chat annotation sweep is P4.D53's — carve out.
                compare_execute(
                    name,
                    &got_body,
                    &want_body,
                    &got_state,
                    &want_state,
                    &literals,
                    got_labels,
                    want_labels,
                    failures,
                );
            } else {
                // Result-only arms (the undefined-data TypeError catch).
                let mut n = Normalizer::new(literals.clone());
                let g = n.value(&mask_result_warnings(&got_body));
                let mut n2 = Normalizer::new(literals);
                let w = n2.value(&mask_result_warnings(body));
                if g != w {
                    failures.push(format!(
                        "[{name}] result differs\n  rust:   {g}\n  oracle: {w}"
                    ));
                }
                let post = read_state(&scratch);
                if post != pre {
                    failures.push(format!("[{name}] a no-write arm MUTATED the database"));
                }
            }
        }
        other => failures.push(format!("[{name}] unexpected response variant: {other:?}")),
    }
}

/// P4.88: the twin of the oracle's `plantP4d171Values` — NON-DEFAULT values in
/// the two `78b381a96` columns, so this family measures the carry rather than
/// two Zod defaults agreeing. The oracle plants the identical cells on v4's
/// copy, so both engines provably start from the same bytes.
fn plant_p4d171_values(conn: &rusqlite::Connection) {
    conn.execute(
        "UPDATE \"chats\" SET \"cycleOrderParticipantIds\" = ?1 WHERE \"id\" = ?2",
        rusqlite::params![
            r#"["e1000000-0000-4000-8000-000000000001","e1000000-0000-4000-8000-0000000000e2"]"#,
            "c1000000-0000-4000-8000-000000000001"
        ],
    )
    .expect("plant the drawn rotation");
    conn.execute(
        "UPDATE \"chat_messages\" SET \"routeTrail\" = ?1 WHERE \"id\" = ?2",
        rusqlite::params![
            r#"[{"profileId":"c0000001-0000-4000-8000-000000000001","profileName":"Primary","provider":"OPENAI","modelName":"gpt-4o","via":"primary","outcome":"failed","trigger":"rate-limit","detail":"429 slow down"},{"profileId":"c0000001-0000-4000-8000-0000000000f2","profileName":"Understudy","provider":"ANTHROPIC","modelName":"claude-x","via":"understudy","outcome":"answered"}]"#,
            "d1000000-0000-4000-8000-000000000002"
        ],
    )
    .expect("plant the route trail");
}
