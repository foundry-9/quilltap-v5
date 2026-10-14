//! P4.D288 (C1-2) — the shared Rust half of the span-ensure families: open the
//! case's ENCRYPTED base file copy, run v5's ensure(s), and inspect exactly as
//! `harness/oracle/cases/p4d288-ensures.ts` inspects v4's copy — the whole
//! `sqlite_master` (bar the two ledger tables), each family table's `PRAGMA
//! table_info` and every row through SQL `quote()`, and the `migrations_state`
//! rows `(id, itemsAffected, message)`.
//!
//! The one normalization: a cursor row's `createdAt` / `updatedAt` minted at
//! run time (anything but the planted `T0`) reads `<ts>` — after asserting the
//! two are equal on that row (v4's and v5's ONE `now`).

#![allow(dead_code)]

use rusqlite::Connection;
use serde_json::{json, Map, Value};

pub const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";
const T0_QUOTED: &str = "'2026-01-01T00:00:00.000Z'";

/// A ruled both-ways mode: v4's report and v5's runs are pinned, the state
/// is NOT compared (they differ by the ruling).
pub struct RuledMode {
    pub name: &'static str,
    pub mode: &'static str,
    /// v4's report, exactly.
    pub v4_report: &'static [&'static str],
    /// The migration ids v5 ran on this mode (empty — nothing).
    pub v5_ran: &'static [&'static str],
}

pub fn open(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let hex = quilltap_core::dbkey::pepper_b64_to_key_hex(PEPPER).expect("pepper");
    conn.execute_batch(&format!("PRAGMA key = \"x'{hex}'\";"))
        .expect("key");
    conn
}

fn table_exists(conn: &Connection, t: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [t],
        |_| Ok(()),
    )
    .is_ok()
}

pub fn inspect(conn: &Connection, tables: &[&str]) -> Value {
    let mut stmt = conn
        .prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_master
          WHERE name NOT IN ('migrations_state', 'migrations_metadata')
            AND tbl_name NOT IN ('migrations_state', 'migrations_metadata')
          ORDER BY type, name",
        )
        .unwrap();
    let master: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "type": r.get::<_, String>(0)?,
                "name": r.get::<_, String>(1)?,
                "tbl_name": r.get::<_, String>(2)?,
                "sql": r.get::<_, Option<String>>(3)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut out_tables = Map::new();
    for t in tables {
        if !table_exists(conn, t) {
            out_tables.insert((*t).to_string(), Value::Null);
            continue;
        }
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info(\"{t}\")"))
            .unwrap();
        let info: Vec<(Value, String)> = stmt
            .query_map([], |r| {
                let name: String = r.get(1)?;
                Ok((
                    json!({
                        "cid": r.get::<_, i64>(0)?,
                        "name": name,
                        "type": r.get::<_, String>(2)?,
                        "notnull": r.get::<_, i64>(3)?,
                        "dflt_value": r.get::<_, Option<String>>(4)?,
                        "pk": r.get::<_, i64>(5)?,
                    }),
                    name,
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let names: Vec<String> = info.iter().map(|(_, n)| n.clone()).collect();
        let cols = names
            .iter()
            .map(|c| format!("quote(\"{c}\")"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut stmt = conn
            .prepare(&format!("SELECT {cols} FROM \"{t}\" ORDER BY rowid"))
            .unwrap();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let mut row = Vec::with_capacity(names.len());
                for i in 0..names.len() {
                    row.push(Value::String(r.get::<_, String>(i)?));
                }
                Ok(Value::Array(row))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        out_tables.insert(
            (*t).to_string(),
            json!({ "info": info.into_iter().map(|(v, _)| v).collect::<Vec<_>>(), "rows": rows }),
        );
    }
    let ledger: Vec<Value> = if table_exists(conn, "migrations_state") {
        let mut stmt = conn
            .prepare("SELECT id, itemsAffected, message FROM migrations_state ORDER BY id")
            .unwrap();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                Ok(json!({
                    "id": r.get::<_, String>(0)?,
                    "itemsAffected": r.get::<_, i64>(1)?,
                    "message": r.get::<_, Option<String>>(2)?,
                }))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows
    } else {
        Vec::new()
    };
    let mut state = json!({ "master": master, "tables": out_tables, "ledger": ledger });
    normalize_cursor_stamps(&mut state);
    state
}

/// The cursor rows' minted `createdAt` / `updatedAt` → `<ts>` (equal on the
/// row first); the planted `T0` stays.
pub fn normalize_cursor_stamps(state: &mut Value) {
    let Some(table) = state["tables"].get_mut("memory_extraction_cursors") else {
        return;
    };
    if table.is_null() {
        return;
    }
    let names: Vec<String> = table["info"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap().to_string())
        .collect();
    let (Some(ci), Some(ui)) = (
        names.iter().position(|n| n == "createdAt"),
        names.iter().position(|n| n == "updatedAt"),
    ) else {
        return;
    };
    for row in table["rows"].as_array_mut().unwrap() {
        let row = row.as_array_mut().unwrap();
        assert_eq!(
            row[ci], row[ui],
            "a cursor row's two stamps differ: {row:?}"
        );
        for i in [ci, ui] {
            if row[i] != Value::String(T0_QUOTED.into()) {
                row[i] = Value::String("<ts>".into());
            }
        }
    }
}

/// The ids v4's report says RAN (`+RAN <id> (…)`).
pub fn v4_ran(report: &Value) -> Vec<String> {
    report
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|l| l.strip_prefix("+RAN "))
        .map(|l| l.split(' ').next().unwrap().to_string())
        .collect()
}

/// Run one family: every oracle row's mode through `ensure` (which answers
/// the ids v5 RAN, in order) on a copy of the case's base file, then compare
/// the state and the ran-ids with v4's. `ruled` modes pin both sides' reports
/// and skip the state compare.
pub fn run_family(
    family: &str,
    tables: &[&str],
    expected_modes: &[&str],
    ruled: &[RuledMode],
    ensure: impl Fn(&Connection) -> Vec<&'static str>,
) -> Option<usize> {
    let var = format!(
        "QT_ORACLE_P4D288_ENSURE_{}",
        family.to_uppercase().replace('-', "_")
    );
    let Ok(oracle) = std::env::var(&var) else {
        eprintln!("SKIP: set {var} to the oracle NDJSON (see header).");
        return None;
    };
    let Ok(dir) = std::env::var("QT_FIXTURE_P4D288_ENSURES_DIR") else {
        eprintln!("SKIP: set QT_FIXTURE_P4D288_ENSURES_DIR to the case's base files (header).");
        return None;
    };
    let text = std::fs::read_to_string(&oracle).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let mut modes = Vec::new();
    let mut failures = Vec::new();
    let mut ruled_met = 0usize;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let mut row: Value = serde_json::from_str(line).expect("oracle line");
        assert_eq!(row["family"], family, "the oracle is another family's");
        // The same normalization on v4's side (and its ONE-`now` assertion).
        normalize_cursor_stamps(&mut row);
        let mode = row["mode"].as_str().unwrap().to_string();
        let scratch = tempfile::Builder::new()
            .prefix("qt-p4d288-ensure-rust-")
            .tempdir()
            .unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(
            std::path::Path::new(&dir)
                .join(family)
                .join(format!("{mode}.db")),
            &work,
        )
        .unwrap_or_else(|e| panic!("copy {mode}: {e}"));
        let conn = open(&work);
        let ran = ensure(&conn);
        let got = inspect(&conn, tables);
        drop(conn);
        if let Some(r) = ruled.iter().find(|r| r.mode == mode) {
            ruled_met += 1;
            let want_report: Vec<Value> = r.v4_report.iter().map(|s| json!(s)).collect();
            assert_eq!(
                row["report"],
                Value::Array(want_report),
                "{family}/{mode}: v4 moved on the ruled row {} (retire or re-rule it)",
                r.name
            );
            assert_eq!(
                ran, r.v5_ran,
                "{family}/{mode}: v5 moved on the ruled row {}",
                r.name
            );
            modes.push(mode);
            continue;
        }
        let v4: Vec<String> = v4_ran(&row["report"]);
        if ran != v4 {
            failures.push(format!("{mode}: ran — v5 {ran:?} vs v4 {v4:?}"));
        }
        for key in ["master", "tables", "ledger"] {
            let (g, w) = (
                serde_json::to_string(&got[key]).unwrap(),
                serde_json::to_string(&row[key]).unwrap(),
            );
            if g != w {
                failures.push(format!("{mode}: {key}\n  v5 {g}\n  v4 {w}"));
            }
        }
        modes.push(mode);
    }
    assert!(
        failures.is_empty(),
        "{family}: {} divergence(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(ruled_met, ruled.len(), "{family}: every ruled row met");
    assert_eq!(modes, expected_modes, "{family}: the modes");
    eprintln!(
        "OK: {family} matched v4's migration(s) in {} mode(s) ({ruled_met} ruled).",
        modes.len()
    );
    Some(modes.len())
}
