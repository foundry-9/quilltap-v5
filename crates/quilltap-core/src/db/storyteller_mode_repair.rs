//! The Storyteller-mode boot ensure — P4.D341, the EIGHTEENTH D23 move (v4
//! `b15fe9404` — migration `add-storyteller-mode-v1`, `dependsOn:
//! ['sqlite-initial-schema-v1']`, `introducedInVersion '4.11.0'`).
//!
//! v4's why, carried (the migration's docblock):
//!
//! - `chats.turnMode` (TEXT, NULL reads as `'rotation'`): `'rotation'` or
//!   `'storyteller'`. Patch-only in the repository.
//! - `chats.storytellerInviteFilter` (INTEGER 0/1, default 0): invite each seat
//!   with a probability weighted by talkativeness instead of inviting everyone.
//! - `chat_messages.storytellerBid` (TEXT JSON, NULL): the bid a storyteller
//!   speaking turn carried out — round, tier, plan, tools named — re-applied on
//!   swipe and regenerate.
//! - `storyteller_rounds`: one row per round while it is open; its bids are
//!   cleared when it closes. Not exported or backed up — bids are not part of
//!   the record.
//!
//! No backfill: every existing chat reads as rotation, and no existing message
//! carried a bid.
//!
//! v4 registers the migration second of the round's last four
//! (`migrations/scripts/index.ts:943` at the `236c04e35` pin); the host runs
//! [`run_storyteller_mode_ensures`] after the keystone ensure and before the
//! llm-logs round-id ensure, in that registration order.
//!
//! ## Shapes
//!
//! The three columns are TWO-shape (generateDDL's `"turnMode" TEXT` /
//! `"storytellerInviteFilter" INTEGER DEFAULT 0` between `isPaused` and
//! `isManuallyRenamed`, and `"storytellerBid" TEXT` between `carinaMeta` and
//! `pascalMeta`, on a FRESH instance; the migration's appended `TEXT DEFAULT
//! NULL` / `INTEGER DEFAULT 0` / `TEXT DEFAULT NULL` on an existing one).
//!
//! The table on an EXISTING instance is the MIGRATION's DDL verbatim (R-2 —
//! bare `PRIMARY KEY`, the three `DEFAULT NULL`s, the `chatId` foreign key
//! with `ON DELETE CASCADE`) plus its `idx_storyteller_rounds_chat`, exactly
//! what v4 does to an existing instance (the P4.D315 `chat_wardrobe_history`
//! pattern). A FRESH instance has generateDDL's table (no FK) from
//! `fresh_schema.json` — the standing R-A provisioned-vs-migrated asymmetry.
//! generateDDL's `idx_storyteller_rounds_createdAt` is made too, READ FROM THE
//! DUMP (never retyped): v4's `ensureCollection` makes it on the repository's
//! first touch, and v5 has no lazy ensure (the `wardrobe_wear_stats_repair`
//! precedent).
//!
//! `HALF_MADE_TABLE_INDEXES` (a v5-only arm, named): when the table already
//! EXISTS but either index is absent, the missing one is made with `IF NOT
//! EXISTS`. v4's migration would not run for the table alone, but its
//! `ensureCollection` makes the createdAt index on first touch and P4.160's
//! backfill makes the chat index from `migration_indexes.json` on the same
//! boot — so the arm only reaches, earlier, the state both apps converge on.
//!
//! ## No stamp, fatal
//!
//! Every arm of v4's `shouldRun` (`chats ∧ chat_messages ∧ (¬turnMode ∨
//! ¬storytellerInviteFilter ∨ ¬storytellerBid ∨ ¬storyteller_rounds)`) is
//! structural: once the ensure ran v4 answers `not needed` and records nothing
//! (`UNSTAMPED_COLUMN_ENSURES`). FATAL on failure (the host `?`s the chain).
//!
//! ## NO-PORT
//!
//! The migration's INFOs `Added chats.turnMode column` / `Added
//! chats.storytellerInviteFilter column` / `Added chat_messages.storytellerBid
//! column` / `Created storyteller_rounds table` (`context:
//! 'migration.add-storyteller-mode-v1'`), its result message `Storyteller mode
//! storage in place (${n} changes)`, its ERROR `Storyteller-mode migration
//! failed`, the DEBUG `Adding column`, the `migrations_state` row and the
//! prettify label `Clearing a dais in the Salon for the Storyteller, and a
//! slate on which the company may bid`. The host logs v5's ONE boot INFO
//! (`Cleared a dais in the Salon for the Storyteller`) when the chain added a
//! column or created the table; silent otherwise.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, index_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-storyteller-mode-v1";
/// The migration's column clauses, verbatim.
pub const TURN_MODE_DECL: &str = "TEXT DEFAULT NULL";
pub const INVITE_FILTER_DECL: &str = "INTEGER DEFAULT 0";
pub const STORYTELLER_BID_DECL: &str = "TEXT DEFAULT NULL";
/// The table the migration creates.
pub const STORYTELLER_ROUNDS_TABLE: &str = "storyteller_rounds";
/// The migration's chat index.
pub const CHAT_INDEX: &str = "idx_storyteller_rounds_chat";
/// generateDDL's createdAt index (read from `fresh_schema.json`).
pub const CREATED_AT_INDEX: &str = "idx_storyteller_rounds_createdAt";

/// The migration's DDL VERBATIM (`add-storyteller-mode-v1.ts:70-86`, R-2) —
/// whitespace included, so `sqlite_master.sql` is byte-equal to v4's.
const CREATE_TABLE_SQL: &str = r#"CREATE TABLE IF NOT EXISTS "storyteller_rounds" (
            "id" TEXT PRIMARY KEY,
            "chatId" TEXT NOT NULL,
            "triggerMessageId" TEXT,
            "status" TEXT NOT NULL,
            "bids" TEXT DEFAULT '[]',
            "selection" TEXT DEFAULT NULL,
            "closedReason" TEXT DEFAULT NULL,
            "createdAt" TEXT NOT NULL,
            "updatedAt" TEXT NOT NULL,
            "closedAt" TEXT DEFAULT NULL,
            FOREIGN KEY ("chatId") REFERENCES "chats"("id") ON DELETE CASCADE
          )"#;
/// The migration's index statement, copied.
const CREATE_CHAT_INDEX_SQL: &str = r#"CREATE INDEX IF NOT EXISTS "idx_storyteller_rounds_chat" ON "storyteller_rounds" ("chatId", "status")"#;

/// What [`run_storyteller_mode_ensures`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StorytellerEnsureOutcome {
    /// 0–3: the columns appended (v4's `itemsAffected` minus the table).
    pub columns_added: u8,
    /// The `storyteller_rounds` table was created (with both indexes).
    pub table_created: bool,
    /// Indexes made on an EXISTING table (`HALF_MADE_TABLE_INDEXES`).
    pub indexes_healed: u8,
}

impl StorytellerEnsureOutcome {
    /// Whether the boot INFO is owed (a column or the table — v4's `run`).
    pub fn changed(&self) -> bool {
        self.columns_added > 0 || self.table_created
    }
}

fn created_at_index_sql() -> Result<String, DbError> {
    let sql = crate::services::provisioning::fresh_main_index(CREATED_AT_INDEX)
        .map_err(|e| DbError::Internal(e.to_string()))?
        .ok_or_else(|| {
            DbError::Internal(format!("fresh_schema.json carries no {CREATED_AT_INDEX}"))
        })?;
    Ok(sql.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1))
}

/// `add-storyteller-mode-v1`: the three columns in v4's order, then the table
/// with its indexes when absent.
pub fn run_storyteller_mode_ensures(
    main: &Connection,
) -> Result<StorytellerEnsureOutcome, DbError> {
    let mut outcome = StorytellerEnsureOutcome::default();
    if !table_exists(main, "chats")? || !table_exists(main, "chat_messages")? {
        return Ok(outcome);
    }
    for (table, column, decl) in [
        ("chats", "turnMode", TURN_MODE_DECL),
        ("chats", "storytellerInviteFilter", INVITE_FILTER_DECL),
        ("chat_messages", "storytellerBid", STORYTELLER_BID_DECL),
    ] {
        if add_column_if_missing(main, table, column, decl)? {
            outcome.columns_added += 1;
        }
    }
    if !table_exists(main, STORYTELLER_ROUNDS_TABLE)? {
        main.execute_batch(CREATE_TABLE_SQL)?;
        main.execute_batch(CREATE_CHAT_INDEX_SQL)?;
        main.execute_batch(&created_at_index_sql()?)?;
        outcome.table_created = true;
    } else {
        // `HALF_MADE_TABLE_INDEXES` (module header).
        if !index_exists(main, CHAT_INDEX)? {
            main.execute_batch(CREATE_CHAT_INDEX_SQL)?;
            outcome.indexes_healed += 1;
        }
        if !index_exists(main, CREATED_AT_INDEX)? {
            main.execute_batch(&created_at_index_sql()?)?;
            outcome.indexes_healed += 1;
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    fn pre_round() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            r#"CREATE TABLE "chats" ("id" TEXT PRIMARY KEY NOT NULL, "isPaused" INTEGER, "isManuallyRenamed" INTEGER);
               CREATE TABLE "chat_messages" ("id" TEXT PRIMARY KEY NOT NULL, "carinaMeta" TEXT, "pascalMeta" TEXT);
               INSERT INTO chats (id) VALUES ('c1');"#,
        )
        .unwrap();
        db
    }

    fn sql_of(db: &Connection, name: &str) -> Option<String> {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .ok()
    }

    #[test]
    fn a_pre_round_instance_gets_everything_in_v4s_order() {
        let db = pre_round();
        let out = run_storyteller_mode_ensures(&db).unwrap();
        assert_eq!(
            out,
            StorytellerEnsureOutcome {
                columns_added: 3,
                table_created: true,
                indexes_healed: 0
            }
        );
        assert!(sql_of(&db, "chats").unwrap().ends_with(
            ", \"turnMode\" TEXT DEFAULT NULL, \"storytellerInviteFilter\" INTEGER DEFAULT 0)"
        ));
        assert!(sql_of(&db, "chat_messages")
            .unwrap()
            .ends_with(", \"storytellerBid\" TEXT DEFAULT NULL)"));
        let table = sql_of(&db, STORYTELLER_ROUNDS_TABLE).unwrap();
        assert!(table
            .contains("FOREIGN KEY (\"chatId\") REFERENCES \"chats\"(\"id\") ON DELETE CASCADE"));
        assert!(
            table.starts_with("CREATE TABLE \"storyteller_rounds\" ("),
            "{table}"
        );
        assert!(sql_of(&db, CHAT_INDEX).is_some());
        assert_eq!(
            sql_of(&db, CREATED_AT_INDEX).as_deref(),
            Some(
                r#"CREATE INDEX "idx_storyteller_rounds_createdAt" ON "storyteller_rounds" ("createdAt" DESC)"#
            )
        );
        let (mode, filter): (Option<String>, i64) = db
            .query_row(
                "SELECT turnMode, storytellerInviteFilter FROM chats",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((mode, filter), (None, 0));
        let again = run_storyteller_mode_ensures(&db).unwrap();
        assert_eq!(again, StorytellerEnsureOutcome::default());
        assert!(!again.changed());
    }

    #[test]
    fn a_half_made_table_gets_its_missing_indexes() {
        let db = pre_round();
        run_storyteller_mode_ensures(&db).unwrap();
        db.execute_batch(
            r#"DROP INDEX "idx_storyteller_rounds_chat"; DROP INDEX "idx_storyteller_rounds_createdAt";"#,
        )
        .unwrap();
        let out = run_storyteller_mode_ensures(&db).unwrap();
        assert_eq!(out.indexes_healed, 2);
        assert!(!out.changed(), "no boot INFO for the heal arm");
    }

    #[test]
    fn no_chats_is_a_no_op() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(r#"CREATE TABLE "chat_messages" ("id" TEXT)"#)
            .unwrap();
        assert_eq!(
            run_storyteller_mode_ensures(&db).unwrap(),
            StorytellerEnsureOutcome::default()
        );
        assert!(sql_of(&db, STORYTELLER_ROUNDS_TABLE).is_none());
        assert_eq!(column_names(&db, "chat_messages").unwrap(), ["id"]);
    }

    #[test]
    fn the_fresh_dump_shape_is_left_alone() {
        let fresh: serde_json::Value =
            serde_json::from_str(include_str!("../services/provisioning/fresh_schema.json"))
                .unwrap();
        let db = Connection::open_in_memory().unwrap();
        for s in fresh["main"].as_array().unwrap() {
            let s = s.as_str().unwrap();
            if s.contains("\"chats\"")
                || s.contains("\"chat_messages\"")
                || s.contains("\"storyteller_rounds\"")
            {
                db.execute_batch(s).unwrap();
            }
        }
        let idx = crate::services::provisioning::migration_main_index(CHAT_INDEX)
            .unwrap()
            .expect("migration_indexes.json carries the chat index");
        assert_eq!(
            idx.replacen("INDEX ", "INDEX IF NOT EXISTS ", 1),
            CREATE_CHAT_INDEX_SQL
        );
        db.execute_batch(&idx).unwrap();
        assert_eq!(
            run_storyteller_mode_ensures(&db).unwrap(),
            StorytellerEnsureOutcome::default()
        );
    }
}
