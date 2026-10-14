//! The structure probes the P4.D288 boot ensures share — v4's
//! `migrations/lib/database-utils.ts` `sqliteTableExists` /
//! `sqliteColumnExists` / `addColumnIfMissing` and the `indexExists` each
//! migration spells inline — so the ten re-homed migrations gate on the same
//! questions in the same words, once.

use rusqlite::Connection;

use super::DbError;

/// v4 `sqliteTableExists(name)`.
pub(crate) fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

/// v4 `sqliteColumnExists(table, column)` — `false` when the table is absent.
pub(crate) fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, DbError> {
    Ok(column_names(conn, table)?.iter().any(|c| c == column))
}

/// `PRAGMA table_info` names, in column order (empty for an absent table).
pub(crate) fn column_names(conn: &Connection, table: &str) -> Result<Vec<String>, DbError> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(names)
}

/// The `SELECT name FROM sqlite_master WHERE type='index' AND name=?` each
/// index migration spells inline.
pub(crate) fn index_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let mut stmt =
        conn.prepare("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1")?;
    Ok(stmt.exists([name])?)
}

/// v4 `addColumnIfMissing(table, column, ddl)` (`database-utils.ts:347-360`):
/// `ALTER TABLE "<table>" ADD COLUMN "<column>" <ddl>` unless the column is
/// there; `true` when it added one. (Its DEBUG `Adding column` belongs to the
/// deferred migration runner — NO-PORT.)
pub(crate) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<bool, DbError> {
    if column_exists(conn, table, column)? {
        return Ok(false);
    }
    conn.execute_batch(&format!(
        "ALTER TABLE \"{table}\" ADD COLUMN \"{column}\" {decl}"
    ))?;
    Ok(true)
}

/// `work` inside ONE savepoint named `name` — a transaction when none is
/// active, nested inside one that is. Released on `Ok`; rolled back (and
/// released) on `Err`, so the ensure lands whole or not at all.
pub(crate) fn in_savepoint<R>(
    conn: &Connection,
    name: &str,
    work: impl FnOnce(&Connection) -> Result<R, DbError>,
) -> Result<R, DbError> {
    conn.execute_batch(&format!("SAVEPOINT \"{name}\""))?;
    match work(conn) {
        Ok(value) => {
            conn.execute_batch(&format!("RELEASE \"{name}\""))?;
            Ok(value)
        }
        Err(error) => {
            let _ = conn.execute_batch(&format!("ROLLBACK TO \"{name}\"; RELEASE \"{name}\""));
            Err(error)
        }
    }
}
