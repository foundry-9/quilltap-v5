//! The `tfidf_vocabularies.textShapeVersion` boot ensure — P4.D288 (C1-2), v4
//! `4ae0c4ca6`'s migration `add-tfidf-text-shape-version-v1` (`dependsOn:
//! ['sqlite-initial-schema-v1', 'create-embedding-tables-v1']`).
//!
//! v4 appends `textShapeVersion INTEGER DEFAULT 0` — the MIGRATION's affinity;
//! generateDDL writes `REAL DEFAULT 0` in the schema slot (R-R: two affinities,
//! every reader takes both — `db::tfidf_vocabulary`). No backfill: 0 is the
//! anchored shape, refit once. [`ensure_tfidf_text_shape_version`] follows v4's
//! `shouldRun` (the table exists and the column is absent) and `run`
//! (`addColumnIfMissing`).
//!
//! Structure-gated, UNSTAMPED, FATAL on failure (the caller `?`s it). v4's
//! migration lines are NO-PORT; the host logs v5's ONE boot INFO when the
//! column was added.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, column_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-tfidf-text-shape-version-v1";

/// The column and the MIGRATION's DDL text (INTEGER — R-R).
pub const COLUMN: &str = "textShapeVersion";
const DECL: &str = "INTEGER DEFAULT 0";

/// Add the column when v4's `shouldRun` would; `true` when it was added.
pub fn ensure_tfidf_text_shape_version(main: &Connection) -> Result<bool, DbError> {
    if !table_exists(main, "tfidf_vocabularies")?
        || column_exists(main, "tfidf_vocabularies", COLUMN)?
    {
        return Ok(false);
    }
    add_column_if_missing(main, "tfidf_vocabularies", COLUMN, DECL)
}
