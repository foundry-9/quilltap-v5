//! Row → v4-entity marshaling for the backup collector (P4.9G5).
//!
//! ## Why this exists rather than reusing the per-family repo reads
//!
//! v4's backup calls each repository's `findAll`/`findByUserId`, which run the
//! generic base-repository path: `SELECT *` → `hydrateRow` (JSON columns parsed,
//! `is*` INTEGER columns coerced to booleans) → the collection adapter turns SQL
//! NULLs into absent keys → Zod `safeParse`. Two consequences fix the wire shape:
//!
//! 1. **Key order is the Zod SCHEMA's declaration order**, not the table's.
//! 2. **A `.nullable().optional()` field whose column is NULL is OMITTED**
//!    entirely (the key never appears), while a `.default(...)` field takes its
//!    default. `JSON.stringify` then drops nothing else.
//!
//! So each entity is described here as an ordered field list, and [`marshal`]
//! walks it. The families whose v5 read already reproduces this exactly (the
//! characters overlay, chats, tags, the two profile kinds, memories, provider
//! models, projects, groups, chat settings) are reused from `db::` instead —
//! this module covers the ones with no v5 reader, or whose v5 reader marshals
//! for a *different* consumer.

use rusqlite::types::ValueRef;
use rusqlite::Row;
use serde_json::{Map, Value};

use crate::db::js_number_to_json;

/// How one column becomes one JSON field.
#[derive(Clone, Copy)]
pub(super) enum F {
    /// Required text.
    Str,
    /// `.nullable().optional()` text — the key is OMITTED when the column is NULL.
    StrOpt,
    /// Required number, rendered the way `JSON.stringify` renders a JS number
    /// (an integer-valued double loses its `.0`).
    Num,
    /// Optional number — omitted when NULL.
    NumOpt,
    /// Required boolean from an INTEGER column.
    Bool,
    /// Optional boolean — omitted when NULL.
    BoolOpt,
    /// A JSON-text column that Zod defaults when absent. The `&str` is the
    /// default's JSON source (`"[]"`, `"{}"`), used when the cell is NULL or
    /// unparseable — matching `fromJsonSafe` + the schema default.
    Json(&'static str),
    /// A JSON-text column that is `.nullable().optional()` — omitted when NULL.
    JsonOpt,
    /// The cell verbatim (text stays text even when it holds JSON — v4 only
    /// parses columns a repository lists as JSON columns).
    Raw,
}

/// Marshal one row into v4's entity object, in the declared field order,
/// omitting the optional fields whose column is NULL.
pub(super) fn marshal(row: &Row<'_>, fields: &[(&str, F)]) -> rusqlite::Result<Value> {
    let mut o = Map::new();
    for (idx, (name, kind)) in fields.iter().enumerate() {
        let cell = row.get_ref(idx)?;
        let is_null = matches!(cell, ValueRef::Null);
        match kind {
            F::Str => {
                o.insert((*name).into(), Value::String(row.get::<_, String>(idx)?));
            }
            F::StrOpt => {
                if !is_null {
                    o.insert((*name).into(), Value::String(row.get::<_, String>(idx)?));
                }
            }
            F::Num => {
                o.insert((*name).into(), js_number_to_json(row.get::<_, f64>(idx)?));
            }
            F::NumOpt => {
                if !is_null {
                    o.insert((*name).into(), js_number_to_json(row.get::<_, f64>(idx)?));
                }
            }
            F::Bool => {
                o.insert((*name).into(), Value::Bool(row.get::<_, i64>(idx)? != 0));
            }
            F::BoolOpt => {
                if !is_null {
                    o.insert((*name).into(), Value::Bool(row.get::<_, i64>(idx)? != 0));
                }
            }
            F::Json(default_src) => {
                let parsed = row
                    .get::<_, Option<String>>(idx)?
                    .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                    .unwrap_or_else(|| {
                        serde_json::from_str::<Value>(default_src).expect("valid default JSON")
                    });
                o.insert((*name).into(), parsed);
            }
            F::JsonOpt => {
                if !is_null {
                    if let Some(v) = row
                        .get::<_, Option<String>>(idx)?
                        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                    {
                        o.insert((*name).into(), v);
                    }
                }
            }
            F::Raw => {
                o.insert((*name).into(), cell_to_value(cell));
            }
        }
    }
    Ok(Value::Object(o))
}

/// A SQLite cell as `JSON.stringify` would render what better-sqlite3 hands JS:
/// NULL → `null`, INTEGER/REAL → number (integer-valued doubles collapse), TEXT →
/// string, BLOB → Node's `Buffer` JSON form `{"type":"Buffer","data":[…]}`.
///
/// The Buffer form only ever shows up in the raw mount-index dumps, where v4
/// runs `SELECT *` with no schema in the way; the two blob columns the backup
/// actually carries (`doc_mount_chunks.embedding`, `doc_mount_blobs.data`) are
/// handled by dedicated code paths before they ever reach here.
pub(super) fn cell_to_value(cell: ValueRef<'_>) -> Value {
    match cell {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => js_number_to_json(f),
        ValueRef::Text(b) => Value::String(String::from_utf8_lossy(b).into_owned()),
        ValueRef::Blob(b) => {
            let mut m = Map::new();
            m.insert("type".into(), Value::String("Buffer".into()));
            m.insert(
                "data".into(),
                Value::Array(b.iter().map(|byte| Value::from(*byte)).collect()),
            );
            Value::Object(m)
        }
    }
}

/// The `SELECT` column list for a field spec, tolerating columns missing from
/// the live table (`crate::db::tolerant_select_list`'s contract: a real,
/// migration-accumulated instance can lack a column v4 never notices because it
/// reads `SELECT *`).
pub(super) fn select_list(
    conn: &rusqlite::Connection,
    table: &str,
    fields: &[(&str, F)],
) -> rusqlite::Result<String> {
    let cols: Vec<&str> = fields.iter().map(|(c, _)| *c).collect();
    crate::db::tolerant_select_list(conn, table, &cols)
}

/// Does the table exist in this partition?
///
/// v4's base repository wraps every read in `safeQuery(..., [])`, so a table
/// that a given instance never provisioned (the legacy `wardrobe_items` on a
/// post-vault-cutover instance is the live example) makes the repo return `[]`
/// rather than throw — and the backup simply carries an empty array. The port
/// reproduces that for the missing-table case specifically; other SQL errors
/// still surface rather than being silently swallowed into an empty backup.
pub(super) fn table_exists(conn: &rusqlite::Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

/// Run a whole-table (or filtered) read through [`marshal`]. `where_sql` is
/// appended verbatim when non-empty (already parameter-safe: callers pass fixed
/// SQL with `?1` placeholders).
pub(super) fn query_all(
    conn: &rusqlite::Connection,
    table: &str,
    fields: &[(&str, F)],
    where_sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Vec<Value>, crate::db::DbError> {
    if !table_exists(conn, table) {
        return Ok(Vec::new());
    }
    let list = select_list(conn, table, fields)?;
    let sql = if where_sql.is_empty() {
        format!("SELECT {list} FROM \"{table}\"")
    } else {
        format!("SELECT {list} FROM \"{table}\" WHERE {where_sql}")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params, |r| marshal(r, fields))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Dump every row of a mount-index table with `SELECT *` semantics — v4
/// `dumpMountIndexTable` (`backup-service.ts:72`). A missing/unreadable table
/// yields `[]` so a backup stays useful on an instance that never provisioned a
/// document store.
pub(super) fn dump_table(conn: &rusqlite::Connection, table: &str) -> Vec<Value> {
    let sql = format!("SELECT * FROM \"{table}\"");
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let mut out = Vec::new();
    let mut rows = match stmt.query([]) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    while let Ok(Some(row)) = rows.next() {
        let mut o = Map::new();
        for (i, name) in names.iter().enumerate() {
            let Ok(cell) = row.get_ref(i) else {
                return out;
            };
            o.insert(name.clone(), cell_to_value(cell));
        }
        out.push(Value::Object(o));
    }
    out
}
