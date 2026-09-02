//! SQLite error classification — the port of v4's `lib/database/sqlite-errors.ts`
//! (v4 `a5df98b3f`, bug 114).
//!
//! Dependency-free predicates for the driver errors that callers legitimately
//! *recover* from rather than propagate. v4 keeps this module free of any import
//! at all so both the database layer and the background-job write applier can
//! share one definition; v5 keeps the same single home, and
//! [`crate::write_partition::is_unique_constraint_error`] — which classifies the
//! same failure arriving as a replayed JSON error shape rather than a live
//! `rusqlite::Error` — reads its message half from here rather than keeping a
//! second copy of the sentence.
//!
//! Recovering from a constraint violation is only ever correct when the losing
//! writer can resolve the *winning* row afterwards — a find-or-create chokepoint
//! ([`crate::db::folders::FoldersRepository::ensure_by_path`]), or the restore
//! loop's quiet duplicate-folder drop.

use super::DbError;

/// The message half of v4's predicate: `/UNIQUE constraint failed/i`.
///
/// Shared with [`crate::write_partition::is_unique_constraint_error`], which
/// answers the same question about a replayed child-write error shape. v4
/// de-duplicated exactly this line when it moved the predicate here.
pub fn message_names_unique_constraint(message: &str) -> bool {
    message.to_lowercase().contains("unique constraint failed")
}

/// True when `err` is a SQLite constraint violation reported by the driver.
///
/// v4 tests `code.startsWith('SQLITE_CONSTRAINT')`, which covers the whole
/// extended-code family (`SQLITE_CONSTRAINT_UNIQUE`,
/// `SQLITE_CONSTRAINT_PRIMARYKEY`, …). rusqlite folds every one of those onto
/// the primary result code, so [`rusqlite::ErrorCode::ConstraintViolation`] is
/// the same set — not a narrowing.
pub fn sqlite_error_is_constraint_violation(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                ..
            },
            _
        )
    )
}

/// v4 `isUniqueConstraintError(err)` — the structured driver code first, then
/// the message a wrapped or re-thrown error carries.
///
/// The message fallback is not redundant: a constraint violation that has
/// already been folded into a [`DbError::Internal`] sentence somewhere up the
/// stack keeps its text but loses its code, exactly as a re-thrown JS `Error`
/// does in v4.
pub fn is_unique_constraint_error(err: &DbError) -> bool {
    if let DbError::Sqlite(e) = err {
        if sqlite_error_is_constraint_violation(e) {
            return true;
        }
    }
    message_names_unique_constraint(&err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table with a UNIQUE index shaped like the one bug 114 adds, so the
    /// predicate is pinned against a REAL driver error rather than a
    /// hand-written string.
    fn conn_with_unique_index() -> rusqlite::Connection {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE t (id TEXT PRIMARY KEY, userId TEXT NOT NULL, \
               projectId TEXT, path TEXT NOT NULL);\
             CREATE UNIQUE INDEX ix ON t (userId, COALESCE(projectId, ''), path);",
        )
        .unwrap();
        c
    }

    fn insert(c: &rusqlite::Connection, id: &str, project: Option<&str>) -> Result<(), DbError> {
        c.execute(
            "INSERT INTO t (id, userId, projectId, path) VALUES (?1, 'u', ?2, '/p/')",
            rusqlite::params![id, project],
        )?;
        Ok(())
    }

    #[test]
    fn a_real_unique_violation_is_recognised() {
        let c = conn_with_unique_index();
        insert(&c, "a", Some("proj")).unwrap();
        let err = insert(&c, "b", Some("proj")).unwrap_err();
        assert!(is_unique_constraint_error(&err), "got {err}");
    }

    #[test]
    fn the_coalesced_null_arm_violates_too() {
        let c = conn_with_unique_index();
        insert(&c, "a", None).unwrap();
        let err = insert(&c, "b", None).unwrap_err();
        assert!(is_unique_constraint_error(&err), "got {err}");
    }

    #[test]
    fn a_primary_key_violation_is_recognised() {
        // v4's `code.startsWith('SQLITE_CONSTRAINT')` covers the whole family,
        // not just UNIQUE — `SQLITE_CONSTRAINT_PRIMARYKEY` is named in its own
        // doc comment. rusqlite folds both onto ConstraintViolation.
        let c = conn_with_unique_index();
        insert(&c, "a", Some("p1")).unwrap();
        let err = insert(&c, "a", Some("p2")).unwrap_err();
        assert!(is_unique_constraint_error(&err), "got {err}");
    }

    #[test]
    fn an_unrelated_sqlite_failure_is_not_a_constraint_error() {
        let c = conn_with_unique_index();
        let err: DbError = c
            .execute("INSERT INTO nope (x) VALUES (1)", [])
            .unwrap_err()
            .into();
        assert!(!is_unique_constraint_error(&err), "got {err}");
    }

    #[test]
    fn a_rethrown_message_still_classifies() {
        // v4's fallback: an error that lost its `code` on the way up keeps its
        // text. v5's analogue is a violation folded into an `Internal` sentence.
        let err =
            DbError::Internal("UNIQUE constraint failed: folders.userId, folders.path".to_string());
        assert!(is_unique_constraint_error(&err));
    }

    #[test]
    fn the_message_test_is_case_insensitive_as_the_regex_is() {
        assert!(message_names_unique_constraint(
            "unique CONSTRAINT Failed: folders.path"
        ));
        assert!(!message_names_unique_constraint("disk I/O error"));
    }

    #[test]
    fn an_unrelated_internal_sentence_is_not_a_constraint_error() {
        assert!(!is_unique_constraint_error(&DbError::Internal(
            "disk full".to_string()
        )));
        assert!(!is_unique_constraint_error(&DbError::WriterGone));
    }
}
