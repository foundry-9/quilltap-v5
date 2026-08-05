//! Shared helpers for the `logLLMCall` regeneration differentials (W4.10b): open
//! a real single-writer `Db` with the `llm_logs` partition materialized, and dump
//! the written rows in the same normalized form the oracle emits (id/createdAt/
//! updatedAt placeholdered, sorted by canonical JSON). Every `*_tier3` differential
//! that un-mocks `logLLMCall` on the v4 side and dumps `llm_logs` uses these so the
//! two dumps line up column-for-column.
//!
//! `#[allow(dead_code)]` — each integration-test binary pulls in this module and
//! uses a different subset of the helpers.
#![allow(dead_code)]

use std::path::Path;

use quilltap_core::db::runtime::{Db, DbPaths};
use quilltap_core::db::Writer;
use serde_json::{Map, Value};

/// The differential test pepper (32 bytes of "testpepper…", base64). Shared with
/// the oracle side so both open valid encrypted DBs (the files are independent —
/// each side has its own DB — but a common pepper keeps the fixtures uniform).
pub const TEST_PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

/// The `llm_logs` DDL — the v4 schema column set/order, matching the oracle's
/// `PRAGMA table_info(llm_logs)` output. Hand-rolled here because `Db::open` never
/// creates tables (the differential harness materializes them, as the tier-2
/// oracles' `generateCreateTable` does on the v4 side).
pub const LLM_LOGS_DDL: &str = "CREATE TABLE llm_logs (\
    id TEXT PRIMARY KEY, userId TEXT, type TEXT, messageId TEXT, \
    chatId TEXT, characterId TEXT, autonomousRunId TEXT, provider TEXT, \
    modelName TEXT, request TEXT, response TEXT, usage TEXT, \
    cacheUsage TEXT, rawProviderUsage TEXT, requestHashes TEXT, \
    durationMs REAL, createdAt TEXT, updatedAt TEXT);";

/// The `llm_logs` columns in schema order.
pub const LLM_LOGS_COLUMNS: &[&str] = &[
    "id",
    "userId",
    "type",
    "messageId",
    "chatId",
    "characterId",
    "autonomousRunId",
    "provider",
    "modelName",
    "request",
    "response",
    "usage",
    "cacheUsage",
    "rawProviderUsage",
    "requestHashes",
    "durationMs",
    "createdAt",
    "updatedAt",
];

/// Materialize the `llm_logs` table on a fresh encrypted DB file (via a throwaway
/// writable open), so `Db::open` can later attach it as the llm-logs partition.
pub fn materialize_llm_logs(path: &Path, pepper: &str) {
    let w = Writer::open_writable(path, pepper).expect("open llm-logs writer");
    w.connection()
        .execute_batch(LLM_LOGS_DDL)
        .expect("create llm_logs table");
}

/// Render a REAL-column `f64` the way JS `JSON.stringify` renders the number
/// better-sqlite3 hands back (an integer-valued float as a bare integer, e.g. the
/// pinned `durationMs: 0`).
fn js_num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
        Value::Number((f as i64).into())
    } else {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

/// Split a dumped `llm_logs` row list into `(byte-faithful, ruled-divergence)`.
///
/// The second half is P4.13 unit 6's RULED deliberate divergence: **v4 logs
/// NOTHING when a cheap-LLM call fails, and v5 writes an error row anyway**
/// (`cheap_llm_exec::log_failed_call` — findings #23/#26 cost hours of
/// invisible failure arms, and v5 has no console to fall back on). No oracle
/// differential is possible for a deliberate divergence, so any family that
/// dumps `llm_logs` over a corpus containing a failed cheap call is
/// permanently red on the row COUNT alone — `compression_tier3` (7 vs 6),
/// which two sweeps counted as an unexplained "content divergence".
///
/// `context_summary_service_tier3`'s old 17-vs-11 escalation is RESOLVED
/// (P4.36, 2026-08-05) and its history corrected: P4.34's "zero error rows"
/// measurement missed them because the signature lives INSIDE the `response`
/// JSON, not in a column. Five of the six extras were a stale oracle mock
/// (the fold-episode extraction prompt had no canned arm, so BOTH sides'
/// passes died — only v5 left a receipt); the sixth is exactly this ruled
/// divergence, and that family now calls this helper legitimately
/// (`context_summary_service_tier3_equivalence.rs`, the `title_failure` op).
///
/// The signature is exact and cannot collide with a success row: v5's success
/// arms always log `response.error = null`, and `log_failed_call` is the only
/// writer that sets it. Callers must assert in BOTH directions (see the
/// `assert_ruled_failed_call_divergence` helper) so the day v4 starts logging
/// these, the pin fails loudly instead of hiding a convergence.
pub fn split_ruled_failed_call_rows(rows: Vec<Value>) -> (Vec<Value>, Vec<Value>) {
    rows.into_iter()
        .partition(|row| !is_ruled_failed_call_row(row))
}

fn is_ruled_failed_call_row(row: &Value) -> bool {
    let Some(Value::String(response)) = row.get("response") else {
        return false;
    };
    serde_json::from_str::<Value>(response)
        .ok()
        .and_then(|v| v.get("error").cloned())
        .is_some_and(|e| !e.is_null())
}

/// Assert the ruled failed-call divergence in both directions, then hand back
/// the rows that MUST match v4 byte-for-byte.
///
/// - v5 must still be writing the error row (else the corpus stopped
///   exercising the failed cheap call and this pin has gone vacuous);
/// - v4 must still be writing none (else the divergence has CONVERGED and the
///   pin must be retired, not silently widened).
pub fn assert_ruled_failed_call_divergence(
    got: Vec<Value>,
    oracle: &[Value],
    family: &str,
) -> Vec<Value> {
    let (got_faithful, got_ruled) = split_ruled_failed_call_rows(got);
    assert!(
        !got_ruled.is_empty(),
        "{family}: v5 wrote no failed-cheap-call error row, so the P4.13 ruled \
         divergence is no longer exercised by this corpus — the pin is vacuous. \
         Either the corpus lost its failing provider arm or `log_failed_call` \
         stopped firing; find out which before relaxing this."
    );
    let oracle_ruled: Vec<&Value> = oracle
        .iter()
        .filter(|r| is_ruled_failed_call_row(r))
        .collect();
    assert!(
        oracle_ruled.is_empty(),
        "{family}: v4 now logs {} failed-cheap-call row(s) too — the P4.13 \
         deliberate divergence has CONVERGED. RETIRE this pin and diff the \
         rows straight, rather than filtering v5's rows away: {oracle_ruled:?}",
        oracle_ruled.len()
    );
    got_faithful
}

/// Dump every `llm_logs` row as a normalized `serde_json::Value` object keyed by
/// column name: id/createdAt/updatedAt collapsed to placeholders, `durationMs`
/// read as REAL (integer-collapsed), every other column as TEXT (NULL → JSON
/// null). Sorted by canonical JSON so the multiset compares regardless of write
/// order — matching the oracle's dump.
pub fn dump_llm_logs(db: &Db) -> Vec<Value> {
    let mut rows: Vec<Value> = db
        .read_llm_logs(|conn| {
            let select = LLM_LOGS_COLUMNS.join(", ");
            let mut stmt = conn.prepare(&format!("SELECT {select} FROM llm_logs"))?;
            let out = stmt
                .query_map([], |row| {
                    let mut m = Map::new();
                    for (i, col) in LLM_LOGS_COLUMNS.iter().enumerate() {
                        let v = if *col == "durationMs" {
                            match row.get::<_, Option<f64>>(i)? {
                                Some(f) => js_num(f),
                                None => Value::Null,
                            }
                        } else {
                            match row.get::<_, Option<String>>(i)? {
                                Some(s) => Value::String(s),
                                None => Value::Null,
                            }
                        };
                        m.insert((*col).to_string(), v);
                    }
                    Ok(Value::Object(m))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(out)
        })
        .expect("read llm_logs");

    for r in &mut rows {
        let m = r.as_object_mut().unwrap();
        m.insert("id".into(), Value::String("<id>".into()));
        m.insert("createdAt".into(), Value::String("<ts>".into()));
        m.insert("updatedAt".into(), Value::String("<ts>".into()));
    }
    sort_by_canonical_json(&mut rows);
    rows
}

/// Sort a row list by its canonical JSON string (the same stable ordering the
/// oracle applies), so a byte-identical multiset lines up index-for-index.
pub fn sort_by_canonical_json(rows: &mut [Value]) {
    rows.sort_by(|a, b| {
        serde_json::to_string(a)
            .unwrap()
            .cmp(&serde_json::to_string(b).unwrap())
    });
}

/// Parse the oracle's `{kind:"llmlogs", columns, rows}` row into its normalized
/// row list, re-sorted by canonical JSON (defensive — the oracle already sorts).
pub fn oracle_llm_logs(v: &Value) -> Vec<Value> {
    let mut rows: Vec<Value> = v["rows"].as_array().expect("llmlogs rows").clone();
    sort_by_canonical_json(&mut rows);
    rows
}

/// Open a fresh main + llm-logs `Db` for a differential that has no seeded main
/// DB (the main file just needs to exist — `is_logging_enabled`'s `chat_settings`
/// read errors on the missing table and defaults to enabled). Returns the handle
/// and the owning `TempDir` (drop it last).
pub fn open_main_and_llm_logs_db(pepper: &str) -> (Db, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let main_path = dir.path().join("main.db");
    let ll_path = dir.path().join("llm-logs.db");
    drop(Writer::open_writable(&main_path, pepper).expect("open main writer"));
    materialize_llm_logs(&ll_path, pepper);
    let db = Db::open(
        DbPaths {
            main: main_path,
            mount_index: None,
            llm_logs: Some(ll_path),
        },
        pepper,
    )
    .expect("open db");
    (db, dir)
}

/// P4.D35 — the four state tiers plus every character's fact sheet, read back
/// through the REAL repositories after a side-effect-bearing run.
///
/// This is what turns the applied-effects claim into a MEASUREMENT:
/// `pascalMeta.effects` says where each write was MEANT to go, and this says
/// where it actually landed. The oracle's `dumpStores` is its twin, reading v4's
/// repositories over the same fixture.
///
/// The character ids are the fixture's three, labelled A/B/C. A broken vault
/// reads as `null` rather than sinking the dump — the fixture's CHAR_D is not
/// in the list, but CHAR_A/B/C can still be read through a partially damaged
/// mount.
pub fn dump_pascal_stores(db: &Db, chat_id: &str, group_id: &str, characters: [&str; 3]) -> Value {
    use quilltap_core::db::characters_read;
    use quilltap_core::db::chats_read;
    use quilltap_core::db::groups::GroupsRepository;
    use quilltap_core::db::projects::ProjectsRepository;
    use quilltap_core::services::mount_index::general_state::read_general_state;

    db.read_main(|main| {
        db.read_mount_index(|mount| {
            let chat = chats_read::find_by_id(main, chat_id)?;
            let chat_state = chat
                .as_ref()
                .and_then(|c| c.get("state").cloned())
                .unwrap_or(Value::Null);
            let project_id = chat
                .as_ref()
                .and_then(|c| c.get("projectId").and_then(Value::as_str))
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            let project_state = match &project_id {
                Some(pid) => ProjectsRepository::new(main, mount)
                    .find_by_id(pid)
                    .ok()
                    .flatten()
                    .and_then(|p| p.get("state").cloned())
                    .unwrap_or(Value::Null),
                None => Value::Null,
            };
            let group_state = GroupsRepository::new(main, mount)
                .find_by_id(group_id)
                .ok()
                .flatten()
                .and_then(|g| g.get("state").cloned())
                .unwrap_or(Value::Null);

            let mut metadata = Map::new();
            for (label, id) in ["A", "B", "C"].iter().zip(characters.iter()) {
                let sheet = characters_read::find_by_id(main, mount, id)
                    .ok()
                    .flatten()
                    .and_then(|c| c.get("metadata").cloned())
                    .unwrap_or(Value::Null);
                metadata.insert((*label).to_string(), sheet);
            }

            let mut out = Map::new();
            out.insert("chat".into(), chat_state);
            out.insert("project".into(), project_state);
            out.insert("group".into(), group_state);
            out.insert("general".into(), read_general_state(main, Some(mount)));
            out.insert("metadata".into(), Value::Object(metadata));
            Ok(Value::Object(out))
        })
    })
    .expect("store dump reads")
}
