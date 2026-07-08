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
