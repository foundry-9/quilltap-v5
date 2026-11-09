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
use serde_json::{json, Map, Value};

use crate::api::zod_issues::{key, zod_issues_joined, zod_parsed_type, ZodIssue};
use crate::db::runtime::Db;
use crate::db::{js_number_to_json, DbError};
use crate::jsstr::{utf16_len, utf16_truncate};
use crate::pascal::js_value::json_stringify;
use crate::tools::llm_number::llm_number;

// === P4.D374 === v4's `logger.child({ context: 'BrahmaSql' })` (`run-sql-handler.ts:30`).
/// The `BrahmaSql` child logger's bindings ride first.
macro_rules! brahma_sql_line {
    ($level:ident, $($rest:tt)*) => {
        tracing::$level!(target: "quilltap::tools", context = "BrahmaSql", $($rest)*)
    };
}

// === /P4.D374 ===

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
        /// P4.D374: v4's `...(fitted.note ? { note } : {})` — serialized LAST,
        /// and only when present.
        note: Option<String>,
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
                note,
            } => {
                // `{ success:true, database, columns, rows, rowCount, truncated,
                // note? }`.
                let mut st = s.serialize_struct("RunSqlResult", 6 + usize::from(note.is_some()))?;
                st.serialize_field("success", &true)?;
                st.serialize_field("database", database)?;
                st.serialize_field("columns", columns)?;
                st.serialize_field("rows", rows)?;
                st.serialize_field("rowCount", &js_number_to_json(*row_count as f64))?;
                st.serialize_field("truncated", truncated)?;
                if let Some(note) = note {
                    st.serialize_field("note", note)?;
                }
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
/// `issues[0]` (`lib/tools/handlers/run-sql-handler.ts:247`).
///
/// The issues are built from the ONE Zod home ([`crate::api::zod_issues`],
/// P4.101) and joined by [`zod_issues_joined`], which is v4's own
/// `` `${i.path.join('.')} — ${i.message}` `` — a renderer over issues, not a
/// second copy of the type.
///
/// ⚠ **Two DOCUMENTED, pre-existing scope limits survive the fold, and P4.101
/// moved no byte on either** (measured 2026-09-18 against v4's REAL
/// `runSqlToolInputSchema` at the `89fcc3c0d` pin, `node --import tsx`):
///
/// 1. The `database` enum's sentence. Real zod 4.5.4 answers
///    `Invalid option: expected one of "main"|"llm-logs"|"mount-index"` with NO
///    `, received <type>` suffix; this port appends one. The
///    `state_sql_tools_equivalence` corpus drives exactly ONE validation case
///    (`sql_validation_nonobject`), so nothing pins it either way. The
///    divergent bytes are kept HERE, at the call site, as a literal variant —
///    correcting them is a wire change that wants its own order with a corpus
///    arm, not a refactor's drive-by (the standing "run the oracle BEFORE
///    changing a user-facing string" rule).
/// 2. `database: null` defaults to `"main"` here where v4 refuses it, and
///    `max_rows` never reports Zod's `>=1` / `<=1000` bounds because the
///    handler clamps instead. Both are behavioural, both predate P4.101, and
///    both are named in the same unwritten order.
///
/// Every OTHER arm — the non-object root and all three `sql` arms — was
/// measured byte-identical to v4 before and after the fold.
fn validate_input(args: &Value) -> Result<RunSqlInput, String> {
    /// v4 renders `issues[0]` only.
    fn refuse(issue: ZodIssue) -> String {
        format!("Invalid run_sql input: {}.", zod_issues_joined(&[issue]))
    }

    let Some(obj) = args.as_object() else {
        // z.object on a non-object: path=[], invalid_type expected object.
        return Err(refuse(ZodIssue::invalid_type("object", vec![], Some(args))));
    };
    // sql: string, min 1.
    let sql = match obj.get("sql") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        Some(Value::String(_)) => {
            return Err(refuse(ZodIssue::too_small_string(
                json!(1),
                vec![key("sql")],
            )))
        }
        other => {
            return Err(refuse(ZodIssue::invalid_type(
                "string",
                vec![key("sql")],
                other,
            )))
        }
    };
    // database: enum, optional (default main).
    let database = match obj.get("database") {
        None | Some(Value::Null) => "main".to_string(),
        Some(Value::String(s)) if DATABASES.contains(&s.as_str()) => s.clone(),
        Some(other) => {
            // Scope limit 1 above: the `, received <type>` suffix is this
            // port's, not zod's. Kept verbatim so P4.101 moves no byte.
            let received = zod_parsed_type(Some(other));
            return Err(refuse(ZodIssue::InvalidValue {
                code: "invalid_value",
                values: DATABASES.iter().map(|v| json!(v)).collect(),
                path: vec![key("database")],
                message: format!(
                    "Invalid option: expected one of \"main\"|\"llm-logs\"|\"mount-index\", received {received}"
                ),
            }));
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
            return Err(refuse(ZodIssue::invalid_type(
                "number",
                vec![key("max_rows")],
                Some(other),
            )))
        }
    };
    Ok(RunSqlInput {
        sql,
        database,
        max_rows,
    })
}

// === P4.D374 === the result size budget (v4 `c539c82e0`, `fitToBudget`).

/// v4 `RESULT_CHAR_BUDGET` — one result's size budget, in UTF-16 units of
/// `JSON.stringify(rows)`. Row caps alone let a handful of wide rows put 30 KB
/// into the context per call; past this budget long text cells are shortened,
/// and if that is not enough, trailing rows are dropped.
pub const RESULT_CHAR_BUDGET: usize = 24_000;
/// v4 `LONG_CELL_CHARS` — what a long text cell is shortened to (UTF-16 units)
/// when the budget is exceeded.
pub const LONG_CELL_CHARS: usize = 600;

/// v4 `fitToBudget`'s return value.
#[derive(Debug, Clone, PartialEq)]
pub struct FitResult {
    pub rows: Vec<Value>,
    pub clipped_cells: usize,
    pub dropped_rows: usize,
    pub note: Option<String>,
}

/// v4 `fitToBudget(rows)` — keep a result inside [`RESULT_CHAR_BUDGET`]:
/// shorten long string cells first (the usual culprit), then drop trailing
/// rows. Returns a note for the model whenever the budget was exceeded, so it
/// knows to narrow the query.
///
/// The size is v4's `JSON.stringify(rows).length` — measured with the JS twin
/// [`json_stringify`] in UTF-16 units (ruling L-4: `serde_json` writes `1e20`
/// where V8 writes `100000000000000000000`, 17 units apart). The note is
/// emitted WHENEVER the budget is exceeded: with one over-budget row and no
/// string cell over 600 its parts list is EMPTY and it reads `Result exceeded
/// the size budget: . Select …` — carried verbatim.
///
/// `RUN_SQL_LONE_SURROGATE_CLIP` (ruling L-3): v4's `value.slice(0, 600)` can
/// split a surrogate pair and keep the lone half; [`utf16_truncate`] decodes
/// U+FFFD there. The size is measured BEFORE the clip, so only the clipped
/// cell's bytes differ, never the drop decision.
pub fn fit_to_budget(rows: Vec<Value>) -> FitResult {
    let size_of = |rows: &[Value]| utf16_len(&json_stringify(&Value::Array(rows.to_vec())));
    if size_of(&rows) <= RESULT_CHAR_BUDGET {
        return FitResult {
            rows,
            clipped_cells: 0,
            dropped_rows: 0,
            note: None,
        };
    }

    let mut clipped_cells = 0usize;
    let clipped: Vec<Value> = rows
        .iter()
        .map(|row| match row {
            Value::Object(obj) => {
                let mut out = Map::new();
                for (k, v) in obj {
                    match v {
                        Value::String(text) if utf16_len(text) > LONG_CELL_CHARS => {
                            out.insert(
                                k.clone(),
                                Value::String(format!(
                                    "{}… [{} chars; select substr({k}, …) to read more]",
                                    utf16_truncate(text, LONG_CELL_CHARS),
                                    utf16_len(text),
                                )),
                            );
                            clipped_cells += 1;
                        }
                        other => {
                            out.insert(k.clone(), other.clone());
                        }
                    }
                }
                Value::Object(out)
            }
            // `Object.entries` of a non-object row: unreachable from
            // `sanitizeRow` (every row is an object); kept as is.
            other => other.clone(),
        })
        .collect();

    let mut kept = clipped;
    let mut size = size_of(&kept);
    while kept.len() > 1 && size > RESULT_CHAR_BUDGET {
        // `kept.slice(0, Math.max(1, Math.floor(kept.length * 0.75)))`.
        let n = ((kept.len() as f64) * 0.75).floor() as usize;
        kept.truncate(n.max(1));
        size = size_of(&kept);
    }
    let dropped_rows = rows.len() - kept.len();

    let mut parts: Vec<String> = Vec::new();
    if clipped_cells > 0 {
        parts.push(format!("{clipped_cells} long text value(s) were shortened"));
    }
    if dropped_rows > 0 {
        parts.push(format!(
            "only the first {} of {} rows fit",
            kept.len(),
            rows.len()
        ));
    }
    FitResult {
        rows: kept,
        clipped_cells,
        dropped_rows,
        note: Some(format!(
            "Result exceeded the size budget: {}. Select fewer or narrower columns, aggregate, or add a LIMIT.",
            parts.join("; ")
        )),
    }
}

// === /P4.D374 ===

/// The `database` enum's members, in schema order.
const DATABASES: &[&str] = &["main", "llm-logs", "mount-index"];

/// Execute the `run_sql` tool (v4 `executeRunSqlTool`). Read-only: runs on a pooled
/// read connection. `user_id` is carried for parity (logging/attribution only).
pub async fn execute_run_sql_tool(db: &Db, args: &Value, user_id: &str) -> RunSqlResult {
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
        // === P4.D374 === v4 `run_sql rejected by pre-scan` (Tier 2 item 20).
        brahma_sql_line!(
            debug,
            userId = %user_id,
            database = %database,
            sqlPreview = %sql_preview(&sql),
            reason = %reason,
            "run_sql rejected by pre-scan"
        );
        return RunSqlResult::fail(reason);
    }

    let db = db.clone();
    let sql_c = sql.clone();
    let db_target = database.clone();
    // Run the whole prepare + read-only guard + fetch inside the read closure. A
    // SQLite prepare/exec error becomes an `Ok(RunSqlResult::fail(..))`; only a
    // partition-unavailable/open error surfaces as `Err(DbError)`.
    let run =
        move |conn: &rusqlite::Connection| -> Result<(RunSqlResult, Option<SqlLog>), DbError> {
            Ok(run_query(conn, &sql_c, &db_target, max_rows))
        };

    let result = match database.as_str() {
        "llm-logs" => db.read_llm_logs(run),
        "mount-index" => db.read_mount_index(run),
        _ => db.read_main(run),
    };

    match result {
        // P4.D374: the query's DEBUG line is emitted HERE, on the caller's
        // thread, after the read closure returns (a pooled read may run its
        // closure elsewhere; the capture rig is thread-scoped).
        Ok((r, log)) => {
            if let Some(log) = log {
                log.emit(user_id, &database, &sql);
            }
            r
        }
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
) -> (RunSqlResult, Option<SqlLog>) {
    // Prepare (also rejects multi-statement SQL authoritatively).
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            let message = sqlite_message(&e);
            return (
                RunSqlResult::fail(format!("SQL error: {message}")),
                Some(SqlLog::PrepareFailed(message)),
            );
        }
    };

    // Layer 2: authoritative read-only guard — fail closed unless exactly true.
    if !stmt.readonly() {
        return (
            RunSqlResult::fail("Only read-only queries are permitted."),
            Some(SqlLog::NotReadOnly(false)),
        );
    }

    let col_count = stmt.column_count();
    let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    // Fetch ALL rows (v4 materializes then slices). A non-reader statement
    // (col_count == 0) yields no rows (v4's `stmt.reader ? all() : []`).
    let mut raw_rows: Vec<Value> = Vec::new();
    if col_count > 0 {
        let exec_failed = |e: &rusqlite::Error| {
            let message = sqlite_message(e);
            (
                RunSqlResult::fail(format!("SQL error: {message}")),
                Some(SqlLog::ExecFailed(message)),
            )
        };
        let mut q = match stmt.query([]) {
            Ok(q) => q,
            Err(e) => return exec_failed(&e),
        };
        loop {
            match q.next() {
                Ok(Some(row)) => raw_rows.push(sanitize_row(row, &col_names)),
                Ok(None) => break,
                Err(e) => return exec_failed(&e),
            }
        }
    }

    let over_cap = raw_rows.len() as i64 > max_rows;
    raw_rows.truncate(max_rows.max(0) as usize);
    // columns = sliced.length > 0 ? Object.keys(sliced[0]) : stmt column names
    // (taken from the SLICED rows, before the budget — the keys are identical).
    let columns: Vec<String> = if let Some(first) = raw_rows.first() {
        first
            .as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    } else {
        col_names
    };

    // === P4.D374 === `fitToBudget(sliced.map(sanitizeRow))`; `truncated =
    // rawRows.length > maxRows || fitted.droppedRows > 0`.
    let fitted = fit_to_budget(raw_rows);
    let truncated = over_cap || fitted.dropped_rows > 0;
    let log = fitted.note.as_ref().map(|_| SqlLog::Shortened {
        clipped_cells: fitted.clipped_cells,
        dropped_rows: fitted.dropped_rows,
    });
    let rows = fitted.rows;
    (
        RunSqlResult::Success {
            database: database.to_string(),
            columns,
            row_count: rows.len(),
            rows,
            truncated,
            note: fitted.note,
        },
        log,
    )
}

// === P4.D374 === v4's `sqlLogger = logger.child({ context: 'BrahmaSql' })`
// lines (`run-sql-handler.ts:30`) — the budget DEBUG and (Tier 2 item 20) the
// four pre-existing rejection DEBUGs v5 had never logged.

/// v4 `SQL_LOG_PREVIEW_CHARS` — how much of the SQL the debug lines echo.
const SQL_LOG_PREVIEW_CHARS: usize = 200;

/// `sql.slice(0, SQL_LOG_PREVIEW_CHARS)` (UTF-16 units).
fn sql_preview(sql: &str) -> String {
    utf16_truncate(sql, SQL_LOG_PREVIEW_CHARS)
}

/// The one DEBUG line a query outcome logs, carried out of the read closure
/// so it is emitted on the caller's thread.
#[derive(Debug, Clone, PartialEq)]
enum SqlLog {
    PrepareFailed(String),
    NotReadOnly(bool),
    ExecFailed(String),
    Shortened {
        clipped_cells: usize,
        dropped_rows: usize,
    },
}

impl SqlLog {
    fn emit(&self, user_id: &str, database: &str, sql: &str) {
        match self {
            SqlLog::PrepareFailed(error) => brahma_sql_line!(
                debug,
                userId = %user_id,
                database = %database,
                sqlPreview = %sql_preview(sql),
                error = %error,
                "run_sql prepare failed"
            ),
            SqlLog::NotReadOnly(readonly) => brahma_sql_line!(
                debug,
                userId = %user_id,
                database = %database,
                sqlPreview = %sql_preview(sql),
                readonly = *readonly,
                "run_sql rejected: statement is not read-only"
            ),
            SqlLog::ExecFailed(error) => brahma_sql_line!(
                debug,
                userId = %user_id,
                database = %database,
                sqlPreview = %sql_preview(sql),
                error = %error,
                "run_sql execution failed"
            ),
            SqlLog::Shortened {
                clipped_cells,
                dropped_rows,
            } => brahma_sql_line!(
                debug,
                userId = %user_id,
                database = %database,
                clippedCells = *clipped_cells as u64,
                droppedRows = *dropped_rows as u64,
                "run_sql result shortened to fit the size budget"
            ),
        }
    }
}
// === /P4.D374 ===

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
            // MEASURED at v4 `f45a517a9` (P4.D203): `f45a517a9` did NOT touch
            // `run-sql-handler.ts`'s `sanitizeRow`, which still renders every
            // BLOB as `<blob: ${value.length} bytes>`. So a compressed
            // `chat_messages.content` reaches the model as a placeholder on
            // BOTH sides, and v4 tells the model nothing about `qt_text`
            // anywhere (zero hits across `lib/tools/`, `lib/services/` and
            // `app/`). Decoding here — or advertising the UDF in the prompt —
            // would be a v5 INVENTION. The model can call `qt_text()` itself;
            // the registration on every connection is what makes that work.
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
            note: None,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"success":true,"database":"main","columns":["a","b"],"rows":[{"a":1,"b":"x"}],"rowCount":1,"truncated":false}"#
        );
    }

    /// P4.D374: v4's `...(fitted.note ? { note } : {})` — the note rides LAST.
    #[test]
    fn success_result_serializes_the_note_last_when_present() {
        let r = RunSqlResult::Success {
            database: "main".into(),
            columns: vec!["a".into()],
            rows: vec![json!({ "a": 1 })],
            row_count: 1,
            truncated: true,
            note: Some("n".into()),
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"success":true,"database":"main","columns":["a"],"rows":[{"a":1}],"rowCount":1,"truncated":true,"note":"n"}"#
        );
    }

    /// P4.D374 `fitToBudget`: under budget is untouched and note-less; over
    /// budget with no long string and one row still notes (the EMPTY parts).
    #[test]
    fn fit_to_budget_arms() {
        let under = fit_to_budget(vec![json!({ "a": "x".repeat(700) })]);
        assert_eq!(
            (under.clipped_cells, under.dropped_rows, under.note),
            (0, 0, None)
        );
        let mut wide = Map::new();
        for k in 0..100 {
            wide.insert(format!("c{k}"), json!("x".repeat(300)));
        }
        let one = fit_to_budget(vec![Value::Object(wide)]);
        assert_eq!((one.clipped_cells, one.dropped_rows), (0, 0));
        assert_eq!(
            one.note.as_deref(),
            Some("Result exceeded the size budget: . Select fewer or narrower columns, aggregate, or add a LIMIT.")
        );
        let big = fit_to_budget(vec![json!({ "t": "y".repeat(30_000) })]);
        assert_eq!(big.clipped_cells, 1);
        assert_eq!(
            big.rows[0]["t"].as_str().unwrap(),
            format!(
                "{}… [30000 chars; select substr(t, …) to read more]",
                "y".repeat(600)
            )
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
