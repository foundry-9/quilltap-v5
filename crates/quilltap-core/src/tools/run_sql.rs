//! Port of v4's `run_sql` tool (Brahma Console read-only SQL) —
//! `lib/tools/handlers/run-sql-handler.ts` + `lib/tools/run-sql-tool.ts`.
//!
//! Runs a single read-only SQL query against one of the three Quilltap databases
//! (main / llm-logs / mount-index) and shapes the rows into a small JSON envelope.
//! The read-only guarantee is **defense in depth**, ported faithfully (v4's exact
//! mechanism, not a "safer" one — the standing don't-harden-mid-port rule):
//!
//!   1. a single-statement + write-keyword pre-scan (literals/comments stripped
//!      first so a `;`/keyword inside a string cannot trip it);
//!   2. the authoritative per-statement read-only check — v4's
//!      `better-sqlite3` `stmt.readonly !== true`; here rusqlite's
//!      [`Statement::readonly`], fail-closed unless exactly `true`. (The v5 read
//!      pool opens the connections **read-only**, an extra layer v4 lacks, but the
//!      per-statement guard is what the differential proves.)
//!   3. a `max_rows` cap on materialized results.
//!
//! Errors are returned as data (`{ success:false, error }`), never thrown, so the
//! model can read the message and self-correct. The SQLite error strings come from
//! the same SQLite3MC engine v4 links, so a prepare/exec error is byte-identical.
//!
//! The operator gate (`operatorSurface`) is a DISPATCHER guard, not part of this
//! handler (v4 `tool-executor.ts:982`): the handler runs the query unconditionally.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::db::runtime::Db;
use crate::db::{js_number_to_json, DbError};
use crate::tools::llm_number::llm_number;

/// Hard upper bound on returned rows, regardless of the requested `max_rows`.
const MAX_ROWS_HARD_CAP: i64 = 1000;
/// Default rows when the model omits `max_rows`.
const DEFAULT_MAX_ROWS: i64 = 200;

/// Write-statement keywords that may never lead or stand alone in a read-only
/// query. Matched as whole words, only when NOT immediately followed by `(` (so
/// scalar `REPLACE(col,'a','b')` is not mistaken for `INSERT OR REPLACE`).
const FORBIDDEN_KEYWORDS: &[&str] = &[
    "INSERT",
    "UPDATE",
    "DELETE",
    "REPLACE",
    "CREATE",
    "DROP",
    "ALTER",
    "TRUNCATE",
    "REINDEX",
    "VACUUM",
    "ATTACH",
    "DETACH",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
];

/// The success envelope (v4 `RunSqlOutput`), or a failure carrying a message. The
/// dispatcher spreads the whole `{ success, .. }` object, so `success` is part of
/// the serialized shape.
#[derive(Debug, Clone, PartialEq)]
pub enum RunSqlResult {
    Success {
        database: String,
        columns: Vec<String>,
        rows: Vec<Value>,
        row_count: usize,
        truncated: bool,
    },
    Failure {
        error: String,
    },
}

impl Serialize for RunSqlResult {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            RunSqlResult::Success {
                database,
                columns,
                rows,
                row_count,
                truncated,
            } => {
                // `{ success:true, database, columns, rows, rowCount, truncated }`.
                let mut st = s.serialize_struct("RunSqlResult", 6)?;
                st.serialize_field("success", &true)?;
                st.serialize_field("database", database)?;
                st.serialize_field("columns", columns)?;
                st.serialize_field("rows", rows)?;
                st.serialize_field("rowCount", &js_number_to_json(*row_count as f64))?;
                st.serialize_field("truncated", truncated)?;
                st.end()
            }
            RunSqlResult::Failure { error } => {
                let mut st = s.serialize_struct("RunSqlResult", 2)?;
                st.serialize_field("success", &false)?;
                st.serialize_field("error", error)?;
                st.end()
            }
        }
    }
}

impl RunSqlResult {
    fn fail(error: impl Into<String>) -> Self {
        RunSqlResult::Failure {
            error: error.into(),
        }
    }
    pub fn success(&self) -> bool {
        matches!(self, RunSqlResult::Success { .. })
    }
}

/// Parsed input (v4 `runSqlToolInputSchema`): `sql` (min 1), `database` (enum,
/// default `main`), `max_rows` (int 1..=1000, default 200). The three optional
/// keys carry their defaults; `.optional()` after `.default()` means an absent key
/// is `undefined`, but the handler applies the same defaults (`?? 'main'` /
/// `?? 200`) so the effect is identical.
struct RunSqlInput {
    sql: String,
    database: String,
    max_rows: i64,
}

/// Validate + normalize the input against the Zod schema, or return the exact
/// `Invalid run_sql input: <path> — <message>.` string v4 builds from
/// `issues[0]`. The corpus constrains invalid inputs to the non-object case (a
/// single, stable Zod message); richer Zod-message fidelity is out of scope
/// (documented — the pre-scan / SQLite-prepare failures cover the real refusals).
fn validate_input(args: &Value) -> Result<RunSqlInput, String> {
    let Some(obj) = args.as_object() else {
        // z.object on a non-object: path=[], invalid_type expected object.
        let received = json_typeof(args);
        return Err(format!(
            "Invalid run_sql input: {}.",
            zod_issue(
                "",
                &format!("Invalid input: expected object, received {received}")
            )
        ));
    };
    // sql: string, min 1.
    let sql = match obj.get("sql") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::String(_)) => {
            return Err(format!(
                "Invalid run_sql input: {}.",
                zod_issue("sql", "Too small: expected string to have >=1 characters")
            ))
        }
        Some(other) => {
            return Err(format!(
                "Invalid run_sql input: {}.",
                zod_issue(
                    "sql",
                    &format!(
                        "Invalid input: expected string, received {}",
                        json_typeof(other)
                    )
                )
            ))
        }
        None => {
            return Err(format!(
                "Invalid run_sql input: {}.",
                zod_issue("sql", "Invalid input: expected string, received undefined")
            ))
        }
    };
    // database: enum, optional (default main).
    let database = match obj.get("database") {
        None | Some(Value::Null) => "main".to_string(),
        Some(Value::String(s)) if s == "main" || s == "llm-logs" || s == "mount-index" => s.clone(),
        Some(other) => {
            let received = json_typeof(other);
            return Err(format!(
                "Invalid run_sql input: {}.",
                zod_issue(
                    "database",
                    &format!("Invalid option: expected one of \"main\"|\"llm-logs\"|\"mount-index\", received {received}")
                )
            ));
        }
    };
    // max_rows: llmNumber(int 1..=1000), optional (default 200). The lenient
    // conversion runs first, so a quoted "50" is validated AND read as 50; a
    // non-numeric value keeps its original type for the Zod error below.
    let max_rows = match obj.get("max_rows").map(|raw| (raw, llm_number(raw))) {
        None => DEFAULT_MAX_ROWS,
        Some((_, coerced)) if coerced.is_null() => DEFAULT_MAX_ROWS,
        Some((_, coerced)) if coerced.is_i64() || coerced.is_u64() => {
            coerced.as_i64().unwrap_or(DEFAULT_MAX_ROWS)
        }
        Some((other, _)) => {
            return Err(format!(
                "Invalid run_sql input: {}.",
                zod_issue(
                    "max_rows",
                    &format!(
                        "Invalid input: expected number, received {}",
                        json_typeof(other)
                    )
                )
            ))
        }
    };
    Ok(RunSqlInput {
        sql,
        database,
        max_rows,
    })
}

/// `${path.join('.')} — ${message}` (path may be empty).
fn zod_issue(path: &str, message: &str) -> String {
    format!("{path} — {message}")
}

/// The JS `typeof`-ish label Zod uses in a `received <x>` message.
fn json_typeof(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Execute the `run_sql` tool (v4 `executeRunSqlTool`). Read-only: runs on a pooled
/// read connection. `user_id` is carried for parity (logging/attribution only).
pub async fn execute_run_sql_tool(db: &Db, args: &Value, _user_id: &str) -> RunSqlResult {
    let input = match validate_input(args) {
        Ok(i) => i,
        Err(e) => return RunSqlResult::fail(e),
    };
    let sql = input.sql.trim().to_string();
    let database = input.database;
    // Math.min(HARD_CAP, Math.max(1, max_rows ?? 200)).
    let max_rows = MAX_ROWS_HARD_CAP.min(1.max(input.max_rows));

    // Layer 1: structural / keyword pre-scan.
    if let Some(reason) = pre_scan_rejection(&sql) {
        return RunSqlResult::fail(reason);
    }

    let db = db.clone();
    let sql_c = sql.clone();
    let db_target = database.clone();
    // Run the whole prepare + read-only guard + fetch inside the read closure. A
    // SQLite prepare/exec error becomes an `Ok(RunSqlResult::fail(..))`; only a
    // partition-unavailable/open error surfaces as `Err(DbError)`.
    let run = move |conn: &rusqlite::Connection| -> Result<RunSqlResult, DbError> {
        Ok(run_query(conn, &sql_c, &db_target, max_rows))
    };

    let result = match database.as_str() {
        "llm-logs" => db.read_llm_logs(run),
        "mount-index" => db.read_mount_index(run),
        _ => db.read_main(run),
    };

    match result {
        Ok(r) => r,
        // v4's `resolveDatabase` returns null → "not available"; our read pool
        // returns PartitionUnavailable for an absent sibling DB.
        Err(DbError::PartitionUnavailable(_)) => RunSqlResult::fail(format!(
            "The {database} database is not available (uninitialized or degraded)."
        )),
        // Any other open/pool failure maps to the same not-available message
        // (v4 treats a null handle uniformly).
        Err(_) => RunSqlResult::fail(format!(
            "The {database} database is not available (uninitialized or degraded)."
        )),
    }
}

/// Prepare, enforce the read-only guard, execute, and shape the rows.
fn run_query(
    conn: &rusqlite::Connection,
    sql: &str,
    database: &str,
    max_rows: i64,
) -> RunSqlResult {
    // Prepare (also rejects multi-statement SQL authoritatively).
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => return RunSqlResult::fail(format!("SQL error: {}", sqlite_message(&e))),
    };

    // Layer 2: authoritative read-only guard — fail closed unless exactly true.
    if !stmt.readonly() {
        return RunSqlResult::fail("Only read-only queries are permitted.");
    }

    let col_count = stmt.column_count();
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    // Fetch ALL rows (v4 materializes then slices). A non-reader statement
    // (col_count == 0) yields no rows (v4's `stmt.reader ? all() : []`).
    let mut raw_rows: Vec<Value> = Vec::new();
    if col_count > 0 {
        let mut q = match stmt.query([]) {
            Ok(q) => q,
            Err(e) => return RunSqlResult::fail(format!("SQL error: {}", sqlite_message(&e))),
        };
        loop {
            match q.next() {
                Ok(Some(row)) => raw_rows.push(sanitize_row(row, &col_names)),
                Ok(None) => break,
                Err(e) => return RunSqlResult::fail(format!("SQL error: {}", sqlite_message(&e))),
            }
        }
    }

    let truncated = raw_rows.len() as i64 > max_rows;
    raw_rows.truncate(max_rows.max(0) as usize);
    let rows = raw_rows;
    // columns = rows.length > 0 ? Object.keys(rows[0]) : stmt column names.
    let columns: Vec<String> = if let Some(first) = rows.first() {
        first
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    } else {
        col_names
    };

    RunSqlResult::Success {
        database: database.to_string(),
        columns,
        row_count: rows.len(),
        rows,
        truncated,
    }
}

/// Build one JSON row object from a rusqlite row, in column order, applying v4's
/// `sanitizeRow`: BLOB → `<blob: N bytes>`, integer → JS number, REAL → JS number
/// (integer-valued collapses), TEXT/NULL passthrough. A duplicate column name
/// collapses last-wins (JS object semantics), keeping the first position.
fn sanitize_row(row: &rusqlite::Row, col_names: &[String]) -> Value {
    let mut obj = Map::new();
    for (i, name) in col_names.iter().enumerate() {
        let value = match row.get_ref(i) {
            Ok(rusqlite::types::ValueRef::Null) => Value::Null,
            Ok(rusqlite::types::ValueRef::Integer(n)) => {
                // better-sqlite3 default (safeIntegers off) → JS number.
                Value::Number(n.into())
            }
            Ok(rusqlite::types::ValueRef::Real(f)) => js_number_to_json(f),
            Ok(rusqlite::types::ValueRef::Text(t)) => {
                Value::String(String::from_utf8_lossy(t).into_owned())
            }
            Ok(rusqlite::types::ValueRef::Blob(b)) => {
                Value::String(format!("<blob: {} bytes>", b.len()))
            }
            Err(_) => Value::Null,
        };
        obj.insert(name.clone(), value);
    }
    Value::Object(obj)
}

/// The SQLite error message text (matches v4's `error.message`, from the same
/// SQLite3MC engine).
fn sqlite_message(e: &rusqlite::Error) -> String {
    match e {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => msg.clone(),
        other => other.to_string(),
    }
}

/// Layer 1 pre-scan (v4 `preScanRejection`). `None` if it passes.
fn pre_scan_rejection(sql: &str) -> Option<String> {
    let code = strip_sql_literals_and_comments(sql);
    let code = code.trim();

    if code.is_empty() {
        return Some("Empty query.".to_string());
    }

    // Reject multiple statements: a `;` followed by more non-whitespace. A single
    // trailing `;` is allowed.
    let without_trailing = strip_trailing_semicolon(code);
    if without_trailing.contains(';') {
        return Some(
            "Only a single statement is permitted; multiple statements are not allowed."
                .to_string(),
        );
    }

    // Leading keyword must be a read-only statement kind.
    if !allowed_leading(without_trailing) {
        return Some(
            "Only read-only queries are permitted (SELECT, WITH … SELECT, EXPLAIN, or a read-only PRAGMA)."
                .to_string(),
        );
    }

    // Reject the mutating PRAGMA assignment form: PRAGMA <name> = <value>.
    if leading_pragma(without_trailing) && pragma_is_assignment(without_trailing) {
        return Some(
            "Mutating PRAGMA assignments are not permitted; only read-only PRAGMAs (e.g. PRAGMA table_info(<table>)) are allowed."
                .to_string(),
        );
    }

    // Reject any write-statement keyword appearing as a standalone word.
    if contains_forbidden_keyword(without_trailing) {
        return Some(
            "Only read-only queries are permitted; write statements are rejected.".to_string(),
        );
    }

    None
}

/// v4 `stripSqlLiteralsAndComments`: replace each string literal / quoted or
/// bracketed identifier / comment with a single space.
fn strip_sql_literals_and_comments(sql: &str) -> String {
    let b = sql.as_bytes();
    let n = b.len();
    let mut out = String::with_capacity(n);
    let mut i = 0;
    while i < n {
        let ch = b[i];
        // Line comment: -- … to EOL.
        if ch == b'-' && i + 1 < n && b[i + 1] == b'-' {
            i += 2;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        // Block comment: /* … */.
        if ch == b'/' && i + 1 < n && b[i + 1] == b'*' {
            i += 2;
            while i < n && !(b[i] == b'*' && i + 1 < n && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            out.push(' ');
            continue;
        }
        // Single-quote string literal (with '' escape).
        if ch == b'\'' {
            i += 1;
            while i < n {
                if b[i] == b'\'' {
                    if i + 1 < n && b[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(' ');
            continue;
        }
        // Double-quote identifier (with "" escape).
        if ch == b'"' {
            i += 1;
            while i < n {
                if b[i] == b'"' {
                    if i + 1 < n && b[i + 1] == b'"' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(' ');
            continue;
        }
        // Backtick identifier.
        if ch == b'`' {
            i += 1;
            while i < n && b[i] != b'`' {
                i += 1;
            }
            i += 1;
            out.push(' ');
            continue;
        }
        // Bracket identifier: [ … ].
        if ch == b'[' {
            i += 1;
            while i < n && b[i] != b']' {
                i += 1;
            }
            i += 1;
            out.push(' ');
            continue;
        }
        out.push(ch as char);
        i += 1;
    }
    out
}

/// `code.replace(/;\s*$/, '')` — drop a single trailing `;` plus trailing space.
fn strip_trailing_semicolon(code: &str) -> &str {
    let trimmed_end = code.trim_end();
    match trimmed_end.strip_suffix(';') {
        Some(prefix) => prefix,
        None => code,
    }
}

/// `/^(SELECT|WITH|EXPLAIN|PRAGMA|VALUES)\b/i`.
fn allowed_leading(code: &str) -> bool {
    let up = leading_word(code).to_ascii_uppercase();
    matches!(
        up.as_str(),
        "SELECT" | "WITH" | "EXPLAIN" | "PRAGMA" | "VALUES"
    )
}

/// `/^PRAGMA\b/i`.
fn leading_pragma(code: &str) -> bool {
    leading_word(code).eq_ignore_ascii_case("PRAGMA")
}

/// `/^PRAGMA\s+[\w.]+\s*=/i` — the mutating-assignment form.
fn pragma_is_assignment(code: &str) -> bool {
    let rest = code.trim_start();
    let Some(after) = rest.get(6..) else {
        return false;
    }; // after "PRAGMA"
    if !after.starts_with(char::is_whitespace) {
        return false;
    }
    let after = after.trim_start();
    // [\w.]+ then optional spaces then '='.
    let mut chars = after.char_indices();
    let mut end = 0;
    let mut saw_name = false;
    for (idx, c) in &mut chars {
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            saw_name = true;
            end = idx + c.len_utf8();
        } else {
            end = idx;
            break;
        }
    }
    if !saw_name {
        return false;
    }
    let tail = after[end..].trim_start();
    tail.starts_with('=')
}

/// The leading identifier word (letters) of `code`, e.g. `"SELECT"` in
/// `"SELECT * ..."`. Used for the `\b`-anchored leading checks.
fn leading_word(code: &str) -> &str {
    let trimmed = code.trim_start();
    let end = trimmed
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(trimmed.len());
    &trimmed[..end]
}

/// `\b(?:INSERT|…)\b(?!\s*\()` case-insensitive — a forbidden write keyword as a
/// standalone word not immediately followed by `(`.
fn contains_forbidden_keyword(code: &str) -> bool {
    let upper = code.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    for kw in FORBIDDEN_KEYWORDS {
        let mut from = 0;
        while let Some(rel) = upper[from..].find(kw) {
            let start = from + rel;
            let end = start + kw.len();
            // Word boundaries: the char before/after must not be a word char.
            let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
            let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
            if before_ok && after_ok {
                // Negative lookahead `(?!\s*\()`: not (optional spaces then `(`).
                let mut j = end;
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }
                let followed_by_paren = j < bytes.len() && bytes[j] == b'(';
                if !followed_by_paren {
                    return true;
                }
            }
            from = start + 1;
        }
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pre_scan_accepts_reads_rejects_writes() {
        assert_eq!(pre_scan_rejection("SELECT 1"), None);
        assert_eq!(pre_scan_rejection("SELECT 1;"), None);
        assert_eq!(
            pre_scan_rejection("  with t as (select 1) select * from t"),
            None
        );
        assert_eq!(pre_scan_rejection("PRAGMA table_info(chats)"), None);
        // Empty.
        assert_eq!(pre_scan_rejection("   ").as_deref(), Some("Empty query."));
        // Multiple statements.
        assert!(pre_scan_rejection("SELECT 1; SELECT 2")
            .unwrap()
            .contains("single statement"));
        // Non-read leading.
        assert!(pre_scan_rejection("DROP TABLE chats")
            .unwrap()
            .contains("read-only queries are permitted (SELECT"));
        // Mutating PRAGMA.
        assert!(pre_scan_rejection("PRAGMA journal_mode = WAL")
            .unwrap()
            .contains("Mutating PRAGMA"));
        // Forbidden keyword inside a CTE.
        assert!(pre_scan_rejection("WITH t AS (SELECT 1) DELETE FROM chats")
            .unwrap()
            .contains("write statements are rejected"));
        // REPLACE( scalar is allowed (followed by paren).
        assert_eq!(
            pre_scan_rejection("SELECT REPLACE(name, 'a', 'b') FROM chats"),
            None
        );
        // A semicolon inside a string literal does not count as multi-statement.
        assert_eq!(pre_scan_rejection("SELECT ';' AS x"), None);
    }

    #[test]
    fn success_result_serializes_in_order() {
        let r = RunSqlResult::Success {
            database: "main".into(),
            columns: vec!["a".into(), "b".into()],
            rows: vec![json!({ "a": 1, "b": "x" })],
            row_count: 1,
            truncated: false,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"success":true,"database":"main","columns":["a","b"],"rows":[{"a":1,"b":"x"}],"rowCount":1,"truncated":false}"#
        );
    }

    #[test]
    fn failure_result_serializes() {
        let r = RunSqlResult::fail("Empty query.");
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"success":false,"error":"Empty query."}"#
        );
    }
}
