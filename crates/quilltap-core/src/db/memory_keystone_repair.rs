//! The keystone-memory boot ensure — P4.D341, the SEVENTEENTH D23 move (v4
//! `8cb584eff` — migration `add-memory-keystone-v1`, `dependsOn:
//! ['add-memory-tiers-v1']`, `introducedInVersion '4.11.0'`).
//!
//! v4's why, carried (the migration's docblock): `memories.keystone` marks a
//! row the operator has set as a standing fact — always delivered to its
//! holder in full and never folded, consolidated or demoted. The partial index
//! on `(characterId, aboutCharacterId)` over keystone rows serves the frozen
//! archive, which reads the keystones once per present character (the holder
//! and each inner-circle member). `ADD COLUMN … DEFAULT 0` stamps every
//! existing row, so there is no backfill loop.
//!
//! v4 registers the migration after the terminal geometry
//! (`migrations/scripts/index.ts:942` at the `236c04e35` pin) — the first of
//! the round's last four. v5's migration runner is a locked deferral, so the
//! column add and the index are re-homed as a boot ensure over the main
//! partition, run by the host after P4.D337's geometry ensure and before the
//! P4.160 index-family backfill. `dependsOn` is satisfied by construction:
//! P4.D265's tiers ensure runs earlier in the same chain.
//!
//! ## TWO shapes (the P4.D249 `chat_informs.permanent` class)
//!
//! A FRESH instance renders generateDDL's `"keystone" INTEGER DEFAULT 0`
//! between `consolidatedAt` and `createdAt` (`MemorySchema.keystone:
//! z.boolean().default(false)` — a default is present, so no `NOT NULL`); those
//! are the bytes of the round's `fresh_schema.json` re-dump, read there, never
//! written here. An EXISTING instance gets the migration's appended `INTEGER
//! NOT NULL DEFAULT 0`. Both read `0` for every pre-existing row.
//!
//! ## The index is this ensure's too (R-4)
//!
//! v4's `shouldRun` is `table ∧ (¬column ∨ ¬index)`. A column-only ensure would
//! leave v4 re-running the migration on the next boot of a shared instance —
//! a ping-pong by construction — so the ensure makes `idx_memories_keystone`
//! itself, v4's statement copied (the `memories_superseded_by_index`
//! precedent), with the existence re-check v4's sibling migrations throw on.
//! ORDER MATTERS: the column BEFORE the index (a `WHERE "keystone" = 1` index
//! on a table without the column fails). The re-dumped `migration_indexes.json`
//! carries the same index for fresh provisioning (SQLite stores both without
//! `IF NOT EXISTS`).
//!
//! ## No stamp, fatal
//!
//! Every arm of v4's `shouldRun` is structural: once the column and the index
//! exist v4 answers `not needed` and records nothing, every boot. So nothing is
//! written to `migrations_state` (the ruled `UNSTAMPED_COLUMN_ENSURES`
//! asymmetry — v4's runner stamps the id only when IT ran). FATAL on failure:
//! the migration is not `resumable`, so a failure stops v4's boot; the host
//! `?`s this ensure.
//!
//! ## NO-PORT
//!
//! Belonging to the deferred migration runner: the INFOs `Added
//! memories.keystone column` / `Created idx_memories_keystone` (`context:
//! 'migration.add-memory-keystone-v1'`), the result message `Keystone memories
//! in place (${itemsAffected} changes)`, the ERROR `Keystone-memory migration
//! failed`, `database-utils`' DEBUG `Adding column`, the `migrations_state`
//! row and `lib/startup/prettify.ts`'s label `Setting keystones in the
//! Commonplace Book, that certain facts never slip`. The host logs v5's ONE
//! boot INFO (`Set the keystones in the Commonplace Book`) when this ensure
//! changed anything, and is silent on a no-op boot.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, index_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-memory-keystone-v1";
/// v4's `INDEX_NAME`.
pub const INDEX_NAME: &str = "idx_memories_keystone";
/// The migration's column clause, verbatim.
pub const KEYSTONE_DECL: &str = "INTEGER NOT NULL DEFAULT 0";
/// v4's statement, copied (`add-memory-keystone-v1.ts:60-62`).
const CREATE_INDEX_SQL: &str = r#"CREATE INDEX IF NOT EXISTS "idx_memories_keystone" ON "memories" ("characterId", "aboutCharacterId") WHERE "keystone" = 1"#;

/// What [`ensure_memory_keystone`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeystoneEnsureOutcome {
    /// v4's `shouldRun` was false — no `memories` table, or the column and the
    /// index both present. Nothing written.
    NotNeeded,
    /// v4's `run`: the column and / or the index made.
    Changed {
        column_added: bool,
        index_created: bool,
    },
}

/// `add-memory-keystone-v1`: append `memories.keystone` when absent, then make
/// the partial index when absent — in that order.
pub fn ensure_memory_keystone(main: &Connection) -> Result<KeystoneEnsureOutcome, DbError> {
    if !table_exists(main, "memories")? {
        return Ok(KeystoneEnsureOutcome::NotNeeded);
    }
    let column_added = add_column_if_missing(main, "memories", "keystone", KEYSTONE_DECL)?;
    let mut index_created = false;
    if !index_exists(main, INDEX_NAME)? {
        main.execute_batch(CREATE_INDEX_SQL)?;
        if !index_exists(main, INDEX_NAME)? {
            return Err(DbError::Internal("Index was not created".into()));
        }
        index_created = true;
    }
    if !column_added && !index_created {
        return Ok(KeystoneEnsureOutcome::NotNeeded);
    }
    Ok(KeystoneEnsureOutcome::Changed {
        column_added,
        index_created,
    })
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    const PRE_ROUND: &str = r#"CREATE TABLE "memories" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "characterId" TEXT NOT NULL,
  "aboutCharacterId" TEXT,
  "consolidatedAt" TEXT,
  "createdAt" TEXT NOT NULL,
  "updatedAt" TEXT NOT NULL
)"#;

    fn db(ddl: &str) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddl).unwrap();
        db.execute(
            "INSERT INTO memories (id, characterId, createdAt, updatedAt) VALUES ('m1', 'c1', 't', 't')",
            [],
        )
        .unwrap();
        db
    }

    fn index_sql(db: &Connection) -> Option<String> {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [INDEX_NAME],
            |r| r.get(0),
        )
        .ok()
    }

    #[test]
    fn a_pre_round_table_gets_the_column_then_the_index() {
        let db = db(PRE_ROUND);
        assert_eq!(
            ensure_memory_keystone(&db).unwrap(),
            KeystoneEnsureOutcome::Changed {
                column_added: true,
                index_created: true
            }
        );
        let sql: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'memories'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            sql.ends_with(", \"keystone\" INTEGER NOT NULL DEFAULT 0)"),
            "{sql}"
        );
        assert_eq!(
            index_sql(&db).as_deref(),
            Some(
                r#"CREATE INDEX "idx_memories_keystone" ON "memories" ("characterId", "aboutCharacterId") WHERE "keystone" = 1"#
            )
        );
        let k: i64 = db
            .query_row("SELECT keystone FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(k, 0, "DEFAULT 0 stamps the existing row");
        assert_eq!(
            ensure_memory_keystone(&db).unwrap(),
            KeystoneEnsureOutcome::NotNeeded
        );
    }

    /// v4's `!hasIndex()` arm — the reason R-4 makes the index here.
    #[test]
    fn the_fresh_shape_without_the_index_gets_the_index_only() {
        let fresh = PRE_ROUND.replace(
            "  \"consolidatedAt\" TEXT,\n",
            "  \"consolidatedAt\" TEXT,\n  \"keystone\" INTEGER DEFAULT 0,\n",
        );
        let db = db(&fresh);
        assert_eq!(
            ensure_memory_keystone(&db).unwrap(),
            KeystoneEnsureOutcome::Changed {
                column_added: false,
                index_created: true
            }
        );
        let names = column_names(&db, "memories").unwrap();
        assert_eq!(names[4], "keystone", "schema position untouched");
    }

    #[test]
    fn no_table_is_a_no_op() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(
            ensure_memory_keystone(&db).unwrap(),
            KeystoneEnsureOutcome::NotNeeded
        );
        assert!(index_sql(&db).is_none());
    }

    /// The re-dumped artifacts carry the two shapes this module documents.
    #[test]
    fn the_committed_dumps_carry_the_fresh_line_and_the_index() {
        let fresh: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let ddl = fresh["main"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|s| {
                s.as_str()
                    .filter(|s| s.starts_with("CREATE TABLE \"memories\""))
            })
            .unwrap();
        assert_eq!(
            ddl.matches(
                "  \"consolidatedAt\" TEXT,\n  \"keystone\" INTEGER DEFAULT 0,\n  \"createdAt\""
            )
            .count(),
            1,
            "{ddl}"
        );
        let idx = crate::services::provisioning::migration_main_index(INDEX_NAME)
            .unwrap()
            .expect("migration_indexes.json carries idx_memories_keystone");
        assert_eq!(
            idx.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1),
            CREATE_INDEX_SQL
        );
    }
}
