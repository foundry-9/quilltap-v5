//! The `chat_messages.isEstablishedFact` boot ensure — P4.D362, the
//! TWENTY-SECOND D23 move (v4 `db6190b44`, migration
//! `add-established-fact-field-v1`, `dependsOn: ['sqlite-initial-schema-v1']`,
//! `introducedInVersion '4.12.0'`).
//!
//! v4's why, carried (the migration's docblock): `isEstablishedFact` (INTEGER
//! 0/1, NULL) is set only on an operator's USER row posted as established fact
//! (the Salon's "Deus Ex Machina"); NULL everywhere else. The frame a model
//! reads is applied at read time and is never stored. No backfill: no existing
//! message was posted as fact.
//!
//! ## The two v4 shapes (the `isSilentMessage` seam exactly)
//!
//! - **generateDDL** (`ChatMessageRowSchema.isEstablishedFact` is a mixed
//!   `z.union([z.boolean(), z.number()…])` → the translator's `'unknown'` →
//!   bare `TEXT`) places `"isEstablishedFact" TEXT` between `isSilentMessage`
//!   and `systemSender` on a FRESH instance — `fresh_schema.json` carries it,
//!   and a written flag lands as TEXT `'1.0'` there;
//! - **the migration** appends `"isEstablishedFact" INTEGER DEFAULT NULL` on an
//!   existing one — INTEGER affinity, so the same write lands `1`.
//!
//! Both are v4's; the write codec is the shared `zero_one_stored`
//! ([`super::chats_messages`]) and the read is the tolerant
//! `put_zero_one_flag` ([`super::chats_messages_read`]).
//!
//! UNSTAMPED (`shouldRun` is column-gated — after this ensure v4 never re-runs
//! it, and a stamp would be a row v4 itself never writes on a migrated
//! instance); FATAL on failure. v4's INFO `Added chat_messages.isEstablishedFact
//! column`, its ERROR `Failed to add isEstablishedFact column`, its result
//! message and its prettify label are NO-PORT (§R.5); the host logs v5's ONE
//! boot INFO when the column was added.

use rusqlite::Connection;

use super::schema_probe::{add_column_if_missing, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-established-fact-field-v1";
/// The migration's column clause, verbatim.
const ESTABLISHED_FACT_DECL: &str = "INTEGER DEFAULT NULL";

/// `add-established-fact-field-v1`: append `isEstablishedFact` when absent.
/// `true` when it added the column; `false` with no `chat_messages` table or
/// with the column already there.
pub fn ensure_chat_messages_established_fact(main: &Connection) -> Result<bool, DbError> {
    if !table_exists(main, "chat_messages")? {
        return Ok(false);
    }
    add_column_if_missing(
        main,
        "chat_messages",
        "isEstablishedFact",
        ESTABLISHED_FACT_DECL,
    )
}

#[cfg(test)]
mod tests {
    use super::super::schema_probe::column_names;
    use super::*;

    #[test]
    fn appends_the_migration_clause_once() {
        let db = Connection::open_in_memory().unwrap();
        assert!(
            !ensure_chat_messages_established_fact(&db).unwrap(),
            "no table"
        );
        db.execute_batch(
            r#"CREATE TABLE "chat_messages" ("id" TEXT PRIMARY KEY, "systemSender" TEXT)"#,
        )
        .unwrap();
        assert!(ensure_chat_messages_established_fact(&db).unwrap());
        assert!(!ensure_chat_messages_established_fact(&db).unwrap());
        assert_eq!(
            column_names(&db, "chat_messages")
                .unwrap()
                .last()
                .map(String::as_str),
            Some("isEstablishedFact")
        );
        let ty: String = db
            .query_row(
                "SELECT type || '|' || coalesce(dflt_value, '') FROM pragma_table_info('chat_messages') WHERE name = 'isEstablishedFact'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ty, "INTEGER|NULL");
    }
}
