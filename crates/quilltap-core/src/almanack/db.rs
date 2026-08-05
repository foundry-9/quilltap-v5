//! The Almanack — database access helpers (v4 `lib/tools/almanack/db.ts`).
//!
//! The report is a census, and a census is a pile of `GROUP BY`. Hydrating whole
//! entities through a schema just to count them is what made v4's old collector
//! slow, so the collectors here read aggregates straight out of SQLite.
//!
//! Three databases are in play and each has its own door — the same three v4
//! names, reached through [`Db`]'s per-partition read pools:
//!   - main (`quilltap.db`)                      → [`main_rows`] / [`main_row`]
//!   - mount index (`quilltap-mount-index.db`)   → [`mount_rows`] / [`mount_row`]
//!   - llm logs (`quilltap-llm-logs.db`)         → [`logs_rows`]
//!
//! Every helper here is *soft*: a failure returns the caller's fallback and logs
//! a warning. A diagnostic report that refuses to render because one of its own
//! queries threw is worse than useless — the section that matters is usually the
//! one next to the broken one.
//!
//! ## The v5 door for the llm-logs aggregates (§2 of the work order)
//!
//! v4 reaches its llm-logs roll-ups through a repository method
//! (`LLMLogsRepository.aggregate`); v5 has no user-scoped repository layer to
//! mirror, so the same raw SQL is issued here, over the llm-logs connection,
//! behind the same fail-soft shape. A structural no-op — the SQL is v4's,
//! byte-for-byte — recorded so the difference is not mistaken for a behaviour
//! change. `db/llm_logs.rs` is deliberately NOT touched (it is P4.D49's).

use rusqlite::types::ValueRef;
use rusqlite::{Connection, Row};

use crate::db::runtime::Db;
use crate::db::DbError;
use crate::jsnum::number_from_str;

/// Which partition a soft query addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    Main,
    MountIndex,
    LlmLogs,
}

/// Run a read against one partition, returning the caller's fallback (and
/// logging) on any failure — including the partition simply not being open
/// (v4's `isMountIndexDegraded()` / `getRawMountIndexDatabase() === null`
/// guards, and the llm-logs equivalents).
fn soft_read<T, F>(db: &Db, partition: Partition, context: &str, fallback: T, f: F) -> T
where
    F: FnOnce(&Connection) -> Result<T, DbError>,
{
    let result = match partition {
        Partition::Main => db.read_main(f),
        Partition::MountIndex => db.read_mount_index(f),
        Partition::LlmLogs => db.read_llm_logs(f),
    };
    match result {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                context,
                error = %e,
                "Almanack query failed; using fallback"
            );
            fallback
        }
    }
}

/// Run a `SELECT` against a partition and map every row, `[]` on any failure.
pub fn query_rows<T, F>(
    db: &Db,
    partition: Partition,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    context: &str,
    map: F,
) -> Vec<T>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    soft_read(db, partition, context, Vec::new(), |conn| {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params, |r| map(r))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
}

/// Run a `SELECT` against the MAIN database, `[]` on any failure
/// (v4 `mainRows`).
pub fn main_rows<T, F>(
    db: &Db,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    context: &str,
    map: F,
) -> Vec<T>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    query_rows(db, Partition::Main, sql, params, context, map)
}

/// Run a `SELECT` against the MAIN database and return the first row
/// (v4 `mainRow`).
pub fn main_row<T, F>(
    db: &Db,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    context: &str,
    map: F,
) -> Option<T>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    main_rows(db, sql, params, context, map).into_iter().next()
}

/// Run a `SELECT` against the MOUNT INDEX database (v4 `mountRows`).
///
/// Deliberately NOT the main connection — that addresses the main DB, and
/// pointing a `doc_mount_*` query at it silently returns nothing (dogfood #16
/// was exactly this mistake elsewhere).
pub fn mount_rows<T, F>(
    db: &Db,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    context: &str,
    map: F,
) -> Vec<T>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    query_rows(db, Partition::MountIndex, sql, params, context, map)
}

/// Run a `SELECT` against the mount index and return the first row
/// (v4 `mountRow`).
pub fn mount_row<T, F>(
    db: &Db,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    context: &str,
    map: F,
) -> Option<T>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    mount_rows(db, sql, params, context, map).into_iter().next()
}

/// Run a `SELECT` against the LLM LOGS database (v4's repository `aggregate`).
pub fn logs_rows<T, F>(
    db: &Db,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
    context: &str,
    map: F,
) -> Vec<T>
where
    F: Fn(&Row<'_>) -> rusqlite::Result<T>,
{
    query_rows(db, Partition::LlmLogs, sql, params, context, map)
}

/// Is the mount index reachable at all? Sections say so rather than showing
/// zeroes (v4 `isMountIndexAvailable`).
pub fn is_mount_index_available(db: &Db) -> bool {
    db.read_mount_index(|conn| {
        // A trivial statement, so "reachable" means the cipher context opened
        // and the file answers — not merely that a path was configured.
        conn.prepare("SELECT 1")?;
        Ok(())
    })
    .is_ok()
}

/// Run a collector, logging and swallowing any failure (v4 `collect`).
///
/// v4's collectors run inside `Promise.all` batches; one throwing would take its
/// whole phase down. Each is wrapped so a single broken section degrades to its
/// fallback shape and the rest of the volume still binds.
pub fn collect<T, F>(name: &str, fallback: T, f: F) -> T
where
    F: FnOnce() -> Result<T, DbError>,
{
    match f() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(collector = name, error = %e, "Almanack collector failed; using fallback");
            fallback
        }
    }
}

/// Coerce a SQLite aggregate (which may arrive as NULL / integer / real / text)
/// to a number — v4's `num()`, including its `Number(string)` arm and its
/// `Number.isFinite` guard.
pub fn num(value: ValueRef<'_>) -> f64 {
    match value {
        ValueRef::Integer(i) => {
            let f = i as f64;
            if f.is_finite() {
                f
            } else {
                0.0
            }
        }
        ValueRef::Real(f) => {
            if f.is_finite() {
                f
            } else {
                0.0
            }
        }
        ValueRef::Text(bytes) => match std::str::from_utf8(bytes) {
            Ok(s) => {
                let parsed = number_from_str(s);
                if parsed.is_finite() {
                    parsed
                } else {
                    0.0
                }
            }
            Err(_) => 0.0,
        },
        // v4's `num(undefined)` / `num(null)` / a Buffer all fall through to 0.
        ValueRef::Null | ValueRef::Blob(_) => 0.0,
    }
}

/// `num()` over a column of a row by index.
pub fn num_col(row: &Row<'_>, idx: usize) -> rusqlite::Result<f64> {
    Ok(num(row.get_ref(idx)?))
}

/// A nullable TEXT column as `Option<String>` — SQL NULL and a non-text value
/// both read as absent (v4's `?? null`).
pub fn opt_text(row: &Row<'_>, idx: usize) -> rusqlite::Result<Option<String>> {
    Ok(match row.get_ref(idx)? {
        ValueRef::Text(b) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    })
}

/// A TEXT column as `String`, empty when NULL. v4's rows carry `string` here by
/// schema, so this only differs on a corrupt row — where an empty label is
/// better than a failed report.
pub fn text(row: &Row<'_>, idx: usize) -> rusqlite::Result<String> {
    Ok(opt_text(row, idx)?.unwrap_or_default())
}

/// `(?, ?, …)` placeholder list; `(NULL)` when the set is empty — v4's
/// `inClause`, whose empty form deliberately matches no row.
pub fn in_clause(ids: &[String]) -> String {
    if ids.is_empty() {
        return "(NULL)".to_string();
    }
    let placeholders = vec!["?"; ids.len()].join(", ");
    format!("({placeholders})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn num_matches_v4_coercion() {
        assert_eq!(num(ValueRef::Integer(7)), 7.0);
        assert_eq!(num(ValueRef::Real(1.5)), 1.5);
        assert_eq!(num(ValueRef::Null), 0.0);
        assert_eq!(num(ValueRef::Blob(&[1, 2])), 0.0);
        assert_eq!(num(ValueRef::Text(b"12")), 12.0);
        // JS `Number("")` is 0, `Number("abc")` is NaN → v4's guard yields 0.
        assert_eq!(num(ValueRef::Text(b"")), 0.0);
        assert_eq!(num(ValueRef::Text(b"abc")), 0.0);
        assert_eq!(num(ValueRef::Real(f64::NAN)), 0.0);
    }

    #[test]
    fn in_clause_matches_v4() {
        assert_eq!(in_clause(&[]), "(NULL)");
        assert_eq!(in_clause(&["a".into()]), "(?)");
        assert_eq!(in_clause(&["a".into(), "b".into()]), "(?, ?)");
    }
}
