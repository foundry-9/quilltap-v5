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
//! Generate the oracle (see `system-import-execute.test.ts`), then:
//!   QT_ORACLE_SYSTEM_IMPORT_EXECUTE=/tmp/oracle-system-import-execute.ndjson \
//!     cargo test -p quilltap-harness --test system_import_state -- --nocapture

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use quilltap_core::api::system_qtap;
use quilltap_core::api::types::{ErrorKind, Response};
use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::services::quilltap_import::{
    execute_import, ConflictStrategy, ImportOptions, QuilltapExport,
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

struct Normalizer {
    literals: HashSet<String>,
    derived_hashes: HashSet<String>,
    minted: BTreeMap<String, String>,
}

impl Normalizer {
    fn new(literals: HashSet<String>) -> Self {
        Normalizer {
            literals,
            derived_hashes: HashSet::new(),
            minted: BTreeMap::new(),
        }
    }

    fn with_derived_hashes(mut self, hashes: HashSet<String>) -> Self {
        self.derived_hashes = hashes;
        self
    }

    fn substitute_embedded(&mut self, s: &str) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(s.len());
        let mut i = 0usize;
        while i < b.len() {
            if i + 36 <= b.len() {
                if let Ok(w) = std::str::from_utf8(&b[i..i + 36]) {
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

fn options_of(options: &Value) -> ImportOptions {
    let strategy = match options.get("conflictStrategy").and_then(Value::as_str) {
        Some("skip") => ConflictStrategy::Skip,
        Some("overwrite") | Some("replace") => ConflictStrategy::Overwrite,
        Some("duplicate") => ConflictStrategy::Duplicate,
        other => panic!("oracle execute case with unexpected strategy {other:?}"),
    };
    ImportOptions {
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
    skip_tables: &HashSet<(&str, &str)>,
) -> (Value, StateDump) {
    let mut n = Normalizer::new(literals.clone()).with_derived_hashes(derived);
    let norm_result = n.value(&mask_result_warnings(result));
    let mut norm_state = BTreeMap::new();
    for (part, tables) in state {
        let mut t = BTreeMap::new();
        for (table, rows) in tables {
            if skip_tables.contains(&(part.as_str(), table.as_str())) {
                continue;
            }
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
                run_execute_case(name, case, &user_id, &mut failures);
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

    assert_eq!(ran, 11, "expected 11 cases, ran {ran}");
    assert!(
        failures.is_empty(),
        "{} import-state difference(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn run_execute_case(name: &str, case: &Value, user_id: &str, failures: &mut Vec<String>) {
    let scratch = fresh_fixture(name);

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
    let opts = options_of(&case["options"]);
    let uid = user_id.to_string();

    let db = open_db(&scratch);
    let result = db
        .write_blocking(move |ws| {
            let main = ws.main().connection();
            let mount = ws.mount_index().expect("fixture has a mount partition");
            execute_import(main, mount.connection(), &uid, &export, &opts).map_err(|e| match e {
                quilltap_core::services::quilltap_import::ImportError::Db(d) => d,
                other => panic!("unexpected parse-side error from execute: {other}"),
            })
        })
        .expect("execute_import ran");
    drop(db);

    let got_state = read_state(&scratch);
    let want_state = state_from_value(&case["state"]);
    let literals = literals_for(&pre, &case["exportData"]);

    compare_execute(
        name,
        &result.to_value(),
        &case["result"],
        &got_state,
        &want_state,
        &literals,
        failures,
    );

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
    failures: &mut Vec<String>,
) {
    // P4.6BK closed the chunk gap in this round, so nothing is skipped here:
    // every table, including `doc_mount_chunks`, is diffed row for row.
    let skip: HashSet<(&str, &str)> = HashSet::new();
    let (got_norm_result, got_norm_state) = normalize_side(
        literals,
        derived_hashes(got_state, literals),
        got_result,
        got_state,
        &skip,
    );
    let (want_norm_result, want_norm_state) = normalize_side(
        literals,
        derived_hashes(want_state, literals),
        want_result,
        want_state,
        &skip,
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
                compare_execute(
                    name,
                    &got_body,
                    body,
                    &got_state,
                    &want_state,
                    &literals,
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
