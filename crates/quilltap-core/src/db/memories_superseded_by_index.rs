//! The `idx_memories_supersededById` boot ensure — P4.D288 (C1-2), v4 `c2795733f`'s
//! migration `add-memories-superseded-by-index-v1` (`dependsOn:
//! ['add-memory-tiers-v1']`): a PARTIAL index for the delete chokepoint's
//! digest scrub (`WHERE "supersededById" IS NOT NULL`).
//!
//! [`ensure_memories_superseded_by_index`] follows v4's `shouldRun` (`memories`
//! exists, `supersededById` exists, the index absent) and `run` — v4's ONE
//! statement, byte for byte, then the existence re-check v4 throws on. It runs
//! at v4's slot 6 in the boot chain; P4.160's index-family backfill later finds
//! the name present (`migration_indexes.json` carries the same statement
//! without `IF NOT EXISTS` — SQLite stores both as the same `sqlite_master.sql`,
//! measured by this ensure's family).
//!
//! Structure-gated, UNSTAMPED, FATAL on failure (the caller `?`s it). v4's
//! migration lines are NO-PORT; the host logs v5's ONE boot INFO when the index
//! was created.

use rusqlite::Connection;

use super::schema_probe::{column_exists, index_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-memories-superseded-by-index-v1";

/// v4's `INDEX_NAME`.
pub const INDEX_NAME: &str = "idx_memories_supersededById";

/// v4's statement, copied.
const CREATE_SQL: &str = r#"CREATE INDEX IF NOT EXISTS "idx_memories_supersededById" ON "memories" ("supersededById") WHERE "supersededById" IS NOT NULL"#;

/// Create the index when v4's `shouldRun` would; `true` when it was created.
pub fn ensure_memories_superseded_by_index(main: &Connection) -> Result<bool, DbError> {
    if !table_exists(main, "memories")?
        || !column_exists(main, "memories", "supersededById")?
        || index_exists(main, INDEX_NAME)?
    {
        return Ok(false);
    }
    main.execute_batch(CREATE_SQL)?;
    if !index_exists(main, INDEX_NAME)? {
        // v4's `throw new Error('Index was not created')`.
        return Err(DbError::Internal("Index was not created".into()));
    }
    Ok(true)
}
