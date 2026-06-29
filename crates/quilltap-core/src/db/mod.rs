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

pub mod connection_profiles;
pub mod conversation_annotations;
pub mod folders;
pub mod help_docs;
pub mod image_profiles;
pub mod prompt_templates;
pub mod provider_models;
pub mod roleplay_templates;
pub mod tags;
pub mod text_replacement_rules;

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

    /// The connection-profiles repository over this writer's connection.
    pub fn connection_profiles(&self) -> connection_profiles::ConnectionProfilesRepository<'_> {
        connection_profiles::ConnectionProfilesRepository::new(&self.conn)
    }

    /// The conversation-annotations repository over this writer's connection.
    pub fn conversation_annotations(
        &self,
    ) -> conversation_annotations::ConversationAnnotationsRepository<'_> {
        conversation_annotations::ConversationAnnotationsRepository::new(&self.conn)
    }

    /// The folders repository over this writer's connection.
    pub fn folders(&self) -> folders::FoldersRepository<'_> {
        folders::FoldersRepository::new(&self.conn)
    }

    /// The provider-models repository over this writer's connection.
    pub fn provider_models(&self) -> provider_models::ProviderModelsRepository<'_> {
        provider_models::ProviderModelsRepository::new(&self.conn)
    }

    /// The roleplay-templates repository over this writer's connection.
    pub fn roleplay_templates(&self) -> roleplay_templates::RoleplayTemplatesRepository<'_> {
        roleplay_templates::RoleplayTemplatesRepository::new(&self.conn)
    }

    /// The tags repository over this writer's connection.
    pub fn tags(&self) -> tags::TagsRepository<'_> {
        tags::TagsRepository::new(&self.conn)
    }

    /// The text-replacement-rules repository over this writer's connection.
    pub fn text_replacement_rules(
        &self,
    ) -> text_replacement_rules::TextReplacementRulesRepository<'_> {
        text_replacement_rules::TextReplacementRulesRepository::new(&self.conn)
    }

    /// The help-docs repository over this writer's connection.
    pub fn help_docs(&self) -> help_docs::HelpDocsRepository<'_> {
        help_docs::HelpDocsRepository::new(&self.conn)
    }

    /// The image-profiles repository over this writer's connection.
    pub fn image_profiles(&self) -> image_profiles::ImageProfilesRepository<'_> {
        image_profiles::ImageProfilesRepository::new(&self.conn)
    }

    /// The prompt-templates repository over this writer's connection.
    pub fn prompt_templates(&self) -> prompt_templates::PromptTemplatesRepository<'_> {
        prompt_templates::PromptTemplatesRepository::new(&self.conn)
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
/// `canonValue`): null explicit, BLOBs as lowercase hex, text as-is, numbers
/// rendered the way the v4 oracle renders them.
///
/// REAL cells go through [`js_number_to_json`]: the oracle reads cells via
/// better-sqlite3, which hands JS a `Number`, and `JSON.stringify` collapses an
/// integer-valued double (`9.0`) to `"9"`. A column with REAL affinity (e.g. a
/// `z.number().int()` column whose values SQLite stores as 8-byte floats) would
/// otherwise serialize as `9.0` on the Rust side and `9` on the oracle side —
/// a spurious diff. Matching JS's number serialization keeps the dumps aligned.
fn cell_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => js_number_to_json(f),
        ValueRef::Text(b) => Value::from(String::from_utf8_lossy(b).into_owned()),
        ValueRef::Blob(b) => Value::from(hex::encode(b)),
    }
}

/// Render an `f64` the way JS `JSON.stringify(number)` does for the cases that
/// occur in a DB cell: an integer-valued finite double serializes without a
/// fractional part (`9.0` -> `9`). We collapse such a value to a JSON integer so
/// a REAL-affinity numeric column matches the oracle byte-for-byte; genuinely
/// fractional values (`9.5`) pass through unchanged, as they do in JS. The i64
/// range guard keeps the cast exact (DB integer columns are small; values beyond
/// it are not integer-collapsed, mirroring nothing we store).
fn js_number_to_json(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
        Value::from(f as i64)
    } else {
        Value::from(f)
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
