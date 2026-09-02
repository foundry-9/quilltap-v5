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
//! Engine-authored sentences inside `warnings` (SQLite constraint text, Zod's
//! union error) are masked after their deterministic prefixes — the standing
//! parser-wording seam; every other warning is compared verbatim.
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
//! fail. The five sentences are compared through `mask_warning`'s quoted
//! families (the quoted item NAME is verbatim; only the validator's own
//! sentence is masked, since Zod's wording is not v5's).
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
//! import carries on to the importers behind it.
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
//! `QUOTED_FAMILIES` mask below folded both to
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
//! connection profiles, and the P4.D135 understudy remap the `.qtap` reconcile
//! pass performs, are differential-covered here for the first time.
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

use quilltap_core::api::system_qtap;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
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

/// Warning families whose subject is quoted: keep through the `": ` delimiter,
/// mask the engine sentence after it.
const QUOTED_FAMILIES: &[&str] = &[
    "Failed to import folder \"",
    "Failed to import document \"",
    "Failed to import blob \"",
    "Failed to import mount point \"",
    "Failed to import character \"",
    "Failed to import chat \"",
    "Failed to import project \"",
    "Failed to import group \"",
    "Failed to import wardrobe item \"",
    "Failed to import plugin data for \"",
    "Failed to import message in chat \"",
    // [P4.D91 → bug 79] The five arms that only logged before v4 `275cd7bc`.
    "Failed to import tag \"",
    "Failed to import roleplay template \"",
    "Failed to import connection profile \"",
    "Failed to import image profile \"",
    "Failed to import embedding profile \"",
];

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

fn mask_warning(w: &str) -> String {
    // Fully deterministic warnings stay verbatim.
    if w.starts_with("Memory references non-existent character ") {
        return w.to_string();
    }
    // [P4.D91 → bug 79] The preflight's refusal wrapper. Its tail is the
    // preflight's own message, which is deterministic for a collision and for
    // the store-unavailable sentence — both are compared VERBATIM. Anything
    // else is an engine sentence and masks like the families below.
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
    for family in QUOTED_FAMILIES {
        if let Some(rest) = w.strip_prefix(family) {
            if let Some(cut) = rest.find("\": ") {
                return format!("{family}{}\": <ENGINE>", &rest[..cut]);
            }
        }
    }
    for family in COLON_FAMILIES {
        if w.starts_with(family) {
            return format!("{family}<ENGINE>");
        }
    }
    if w.starts_with("Failed to link project ") {
        if let Some(cut) = w.rfind(": ") {
            return format!("{}: <ENGINE>", &w[..cut]);
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
                    execute_import(main, mount.connection(), &uid, &export, &opts, None)
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
    assert_eq!(ran, 37, "expected 37 cases, ran {ran}");
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
        failures.is_empty(),
        "{} import-state difference(s):\n{}",
        failures.len(),
        failures.join("\n\n")
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
            let r1 = execute_import(main, mount.connection(), &uid, &first, &opts, None)
                .expect("first import");
            let r2 = execute_import(main, mount.connection(), &uid, &second, &opts, None)
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
            other => panic!("[{name}] unknown prep {other}"),
        }
    }

    // The PRE-import baseline: both sides copy the same fixture bytes, so the
    // raw dumps must be EQUAL — anything else is dump-machinery drift, reported
    // as such rather than leaking into every table diff below.
    let pre = read_state(&scratch);
    let want_pre = state_from_value(&case["preState"]);
    if pre != want_pre {
        diff_states(&format!("{name} BASELINE"), &pre, &want_pre, failures);
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

    let db = open_db(&scratch);
    let results = db
        .write_blocking(move |ws| {
            let main = ws.main().connection();
            let mount = ws.mount_index().expect("fixture has a mount partition");
            let mut out = Vec::new();
            for i in 0..runs {
                let opts = if i == 0 { &opts } else { &opts2 };
                out.push(
                    execute_import(main, mount.connection(), &uid, &export, opts, None).map_err(
                        |e| match e {
                            quilltap_core::services::quilltap_import::ImportError::Db(d) => d,
                            other => {
                                panic!("unexpected parse-side error from execute: {other}")
                            }
                        },
                    )?,
                );
            }
            Ok(out)
        })
        .expect("execute_import ran");
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
