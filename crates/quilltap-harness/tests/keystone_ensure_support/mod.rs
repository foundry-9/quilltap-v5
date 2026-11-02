//! P4.D341 — the Rust twin of `harness/oracle/lib/keystone-ensure-kit.ts`: the
//! per-table dumps the round's four NEW ensure families compare byte-for-byte
//! with the oracle (`PRAGMA table_info`, `sqlite_master.sql`, `quote()` /
//! `typeof()` cells, the `migrations_state` ids).
#![allow(dead_code)]

use rusqlite::Connection;
use serde_json::{json, Map, Value};

/// The kit's `PEPPER`.
pub const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

pub fn table_sql(conn: &Connection, name: &str) -> Option<String> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get(0),
    )
    .ok()
}

pub fn index_sql(conn: &Connection, name: &str) -> Option<String> {
    conn.query_row(
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |r| r.get(0),
    )
    .ok()
}

fn names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

pub fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    names(conn, table).iter().any(|n| n == column)
}

/// better-sqlite3's `PRAGMA table_info` row objects, in its key order.
pub fn table_info(conn: &Connection, table: &str) -> Value {
    if table_sql(conn, table).is_none() {
        return json!([]);
    }
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{table}\")"))
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            Ok(json!({
                "cid": r.get::<_, i64>(0)?,
                "name": r.get::<_, String>(1)?,
                "type": r.get::<_, String>(2)?,
                "notnull": r.get::<_, i64>(3)?,
                "dflt_value": r.get::<_, Option<String>>(4)?,
                "pk": r.get::<_, i64>(5)?,
            }))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

/// The kit's `cells`: `id` + each column's `quote()` and `typeof()`.
pub fn cells(conn: &Connection, table: &str, columns: &[&str]) -> Value {
    if table_sql(conn, table).is_none() {
        return json!([]);
    }
    let present = names(conn, table);
    let mut parts = vec!["id".to_string()];
    for c in columns {
        if present.iter().any(|n| n == c) {
            parts.push(format!("quote(\"{c}\")"));
            parts.push(format!("typeof(\"{c}\")"));
        } else {
            parts.push("'absent'".into());
            parts.push("'absent'".into());
        }
    }
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM \"{table}\" ORDER BY id",
            parts.join(", ")
        ))
        .unwrap();
    let rows: Vec<Value> = stmt
        .query_map([], |r| {
            let mut m = Map::new();
            m.insert("id".into(), json!(r.get::<_, String>(0)?));
            for (i, c) in columns.iter().enumerate() {
                m.insert((*c).into(), json!(r.get::<_, String>(1 + 2 * i)?));
                m.insert(format!("{c}Type"), json!(r.get::<_, String>(2 + 2 * i)?));
            }
            Ok(Value::Object(m))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    Value::Array(rows)
}

pub fn ledger(conn: &Connection) -> Vec<String> {
    if table_sql(conn, "migrations_state").is_none() {
        return Vec::new();
    }
    let mut stmt = conn
        .prepare("SELECT id FROM migrations_state ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// v4's report lines as strings.
pub fn report(v4: &Value) -> Vec<String> {
    v4["report"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap().to_string())
        .collect()
}

/// Byte-compare two JSON values (serde's `Map ==` ignores key order).
pub fn assert_bytes(got: &Value, want: &Value, what: &str) {
    assert_eq!(
        serde_json::to_string(got).unwrap(),
        serde_json::to_string(want).unwrap(),
        "{what}"
    );
}

pub fn read_ndjson(path: &std::ffi::OsStr) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
