//! The cipher-correct DB layer (Phase 2).
//!
//! Opens the exact encrypted SQLite files v4 writes — **ChaCha20/sqleet** via
//! the SQLite3MultipleCiphers amalgamation linked in `build.rs`, NOT SQLCipher
//! (see the crate-level cipher note and CLAUDE.md). The pepper is turned into a
//! raw-hex key and applied as `PRAGMA key = "x'<hex>'"`, the first pragma before
//! any other operation; the KDF is skipped because the pepper was already
//! derived when `dbkey` unwrapped the `.dbkey` file.
//!
//! ## Single-writer model
//!
//! Only [`Writer`] holds the read-write connection, and mutations happen only
//! through its methods — the ownership half of v4's parent-is-sole-writer rule.
//! The async actor (a channel as the only mutator, per-database partitioned
//! apply) is layered on in Phase 3 per `docs/developer/porting/api-boundary.md`;
//! this pilot drives repos directly against the owned connection.

use std::path::Path;

use rusqlite::types::ValueRef;
use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::dbkey;

pub mod folders;

/// Errors from the DB layer.
#[derive(Debug)]
pub enum DbError {
    /// Deriving the raw-hex key from the pepper failed.
    Key(String),
    /// A SQLite operation failed.
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Key(msg) => write!(f, "key derivation failed: {msg}"),
            DbError::Sqlite(e) => write!(f, "sqlite error: {e}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sqlite(e)
    }
}

/// The sole holder of a read-write connection to one Quilltap database.
pub struct Writer {
    conn: Connection,
}

impl Writer {
    /// Open an existing encrypted database read-write, matching v4's writable
    /// open sequence byte-for-byte:
    ///   1. `PRAGMA key = "x'<hex>'"` — first and only pragma before any read;
    ///   2. `PRAGMA foreign_keys = ON`;
    ///   3. `PRAGMA journal_mode = TRUNCATE` (TRUNCATE, **not** WAL — cloud-sync
    ///      safety, since instances live in iCloud/Dropbox).
    ///
    /// `pepper_b64` is the base64 pepper (as `dbkey::load_pepper` yields it).
    pub fn open_writable(path: &Path, pepper_b64: &str) -> Result<Self, DbError> {
        let key_hex =
            dbkey::pepper_b64_to_key_hex(pepper_b64).map_err(|e| DbError::Key(e.to_string()))?;

        let conn = Connection::open(path)?;
        // Key MUST be the first pragma. Raw-hex form -> KDF skipped.
        conn.pragma_update(None, "key", format!("x'{key_hex}'"))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // journal_mode returns the resulting mode as a row; consume it.
        let _mode: String =
            conn.query_row("PRAGMA journal_mode = TRUNCATE", [], |row| row.get(0))?;

        Ok(Self { conn })
    }

    /// The folders repository over this writer's connection.
    pub fn folders(&self) -> folders::FoldersRepository<'_> {
        folders::FoldersRepository::new(&self.conn)
    }

    /// Canonical dump of one table, in the same shape the tier-2 oracle emits:
    /// `{ table, columns, rows }` with columns in on-disk order, rows sorted by
    /// `order_by`, BLOBs as lowercase hex, nulls explicit. This is the
    /// structural snapshot the differential harness diffs.
    pub fn dump_table_json(&self, table: &str, order_by: &str) -> Result<Value, DbError> {
        let mut stmt = self.conn.prepare(&format!("SELECT * FROM {table}"))?;
        let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let cols_for_rows = columns.clone();
        let mut rows: Vec<Map<String, Value>> = stmt
            .query_map([], |row| {
                let mut obj = Map::new();
                for (i, col) in cols_for_rows.iter().enumerate() {
                    obj.insert(col.clone(), cell_to_json(row.get_ref(i)?));
                }
                Ok(obj)
            })?
            .collect::<Result<_, _>>()?;

        rows.sort_by_key(|row| order_key(row, order_by));

        Ok(Value::Object({
            let mut m = Map::new();
            m.insert("table".into(), Value::from(table));
            m.insert(
                "columns".into(),
                Value::from(
                    columns
                        .iter()
                        .map(|c| Value::from(c.as_str()))
                        .collect::<Vec<_>>(),
                ),
            );
            m.insert(
                "rows".into(),
                Value::from(rows.into_iter().map(Value::Object).collect::<Vec<_>>()),
            );
            m
        }))
    }
}

/// Convert a SQLite cell to its canonical JSON form (mirrors the TS
/// `canonValue`): null explicit, BLOBs as lowercase hex, text/numbers as-is.
fn cell_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(b) => Value::from(String::from_utf8_lossy(b).into_owned()),
        ValueRef::Blob(b) => Value::from(hex::encode(b)),
    }
}

/// Stable sort key for a row's `order_by` column: the raw string for text,
/// the JSON rendering otherwise, empty for null/missing (matches the TS
/// `String(a[orderBy] ?? '')`).
fn order_key(row: &Map<String, Value>, col: &str) -> String {
    match row.get(col) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}
