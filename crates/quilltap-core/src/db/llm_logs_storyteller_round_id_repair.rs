//! The llm-logs round-id boot ensure — P4.D341, the NINETEENTH D23 move and the
//! FIRST column ensure over the llm-logs partition (v4 `b15fe9404` — migration
//! `add-llm-logs-storyteller-round-id-column-v1`, FILE
//! `add-llm-logs-storyteller-round-id-column.ts` (no `-v1` in the file name),
//! `dependsOn: ['add-llm-logs-autonomous-run-id-column-v1']`,
//! `introducedInVersion '4.11.0'`).
//!
//! v4's why, carried: a nullable `storytellerRoundId` (plus an index) on
//! `llm_logs` in the dedicated llm-logs database. Every LLM call made for a
//! storyteller round — each bid, and each speaking turn the round runs — is
//! stamped with the round's id, so the inspector can show a round as one
//! group. Losing bids are recorded only here. The index supports the per-run
//! budget sum (`WHERE storytellerRoundId = ?`).
//!
//! v4's migration opens the logs FILE itself (`SQLITE_LLM_LOGS_PATH` or
//! `<data>/quilltap-llm-logs.db`); its `shouldRun` answers false when the file
//! is absent ("fresh installs create the table from the Zod-derived DDL on
//! first access"), when `llm_logs` is absent, or when the column exists. v5
//! takes the partition's writer connection instead: the host passes
//! `ws.llm_logs()`, and an Absent or Degraded partition (P4.159's ladder — the
//! v5 analogue of the absent file) SKIPS with a DEBUG, never failing the boot
//! (R-17). It runs inside `seed_built_ins`, i.e. BEFORE PHASE 3.1's structural
//! verify, so a pre-round instance's `llm_logs` is not named MISSING
//! `storytellerRoundId` on its first boot.
//!
//! ## Shapes
//!
//! TWO-shape column: generateDDL's `"storytellerRoundId" TEXT` after
//! `autonomousRunId` on a FRESH instance; the migration's appended `TEXT
//! DEFAULT NULL` on an existing one. The index is made UNCONDITIONALLY once the
//! migration runs (`IF NOT EXISTS`, v4 `:105`) — but it is NOT in v4's
//! `shouldRun` (`!column` only), so a table that HAS the column without the
//! index is `not needed` on both apps and keeps no index here
//! (`COLUMN_WITHOUT_INDEX_NOT_NEEDED`): the fresh path's index comes from
//! `migration_indexes.json` (P4.160's backfill), never from this ensure.
//!
//! ## No stamp, fatal on a present writer
//!
//! UNSTAMPED (`shouldRun` is column-gated — v4 never re-runs it once the column
//! exists). An `Err` from a present writer is FATAL (the host `?`s it).
//!
//! ## NO-PORT
//!
//! The migration's INFO `Added storytellerRoundId column to llm_logs`
//! (`context: 'migration.add-llm-logs-storyteller-round-id-column'` — no
//! `-v1`), its result messages (`Added storytellerRoundId column + index to
//! llm_logs`, `llm_logs table does not exist; nothing to do`), its ERROR
//! `Failed to add storytellerRoundId column to llm_logs`, the
//! `migrations_state` row, and the prettify label. Its migration-time opener's
//! `journal_mode = WAL` is NOT mirrored (v5's partitions open TRUNCATE; the
//! measured header flip is the both-ways carve `V4_LLM_LOGS_MIGRATION_WAL` in
//! the family). The host logs v5's ONE boot INFO (`Taught the wire ledger to
//! bundle each storyteller round under one ribbon`) when the column was added;
//! silent otherwise.

use rusqlite::Connection;

use super::schema_probe::{column_exists, index_exists, table_exists};
use super::DbError;

/// v4's migration id.
pub const MIGRATION_ID: &str = "add-llm-logs-storyteller-round-id-column-v1";
/// v4's index name.
pub const INDEX_NAME: &str = "idx_llm_logs_storytellerRoundId";
/// The migration's column clause, verbatim (`ALTER TABLE "llm_logs" ADD COLUMN
/// "storytellerRoundId" TEXT DEFAULT NULL`).
pub const ROUND_ID_DECL: &str = "TEXT DEFAULT NULL";
/// v4's index statement, copied.
const CREATE_INDEX_SQL: &str = r#"CREATE INDEX IF NOT EXISTS "idx_llm_logs_storytellerRoundId" ON "llm_logs" ("storytellerRoundId")"#;

/// What [`ensure_llm_logs_storyteller_round_id`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmLogsRoundIdOutcome {
    /// `shouldRun` false: no `llm_logs` table, or the column present.
    NotNeeded,
    /// The column was appended; `index_created` from a before / after probe.
    Added { index_created: bool },
}

/// `add-llm-logs-storyteller-round-id-column-v1` over the llm-logs partition's
/// connection.
pub fn ensure_llm_logs_storyteller_round_id(
    llm_logs: &Connection,
) -> Result<LlmLogsRoundIdOutcome, DbError> {
    if !table_exists(llm_logs, "llm_logs")?
        || column_exists(llm_logs, "llm_logs", "storytellerRoundId")?
    {
        return Ok(LlmLogsRoundIdOutcome::NotNeeded);
    }
    llm_logs.execute_batch(&format!(
        "ALTER TABLE \"llm_logs\" ADD COLUMN \"storytellerRoundId\" {ROUND_ID_DECL}"
    ))?;
    let had_index = index_exists(llm_logs, INDEX_NAME)?;
    llm_logs.execute_batch(CREATE_INDEX_SQL)?;
    Ok(LlmLogsRoundIdOutcome::Added {
        index_created: !had_index && index_exists(llm_logs, INDEX_NAME)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRE_ROUND: &str = r#"CREATE TABLE "llm_logs" (
  "id" TEXT PRIMARY KEY NOT NULL,
  "autonomousRunId" TEXT,
  "createdAt" TEXT NOT NULL
)"#;

    fn sql_of(db: &Connection, name: &str) -> Option<String> {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name = ?1",
            [name],
            |r| r.get(0),
        )
        .ok()
    }

    #[test]
    fn a_pre_round_table_gets_the_column_and_the_index() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(PRE_ROUND).unwrap();
        assert_eq!(
            ensure_llm_logs_storyteller_round_id(&db).unwrap(),
            LlmLogsRoundIdOutcome::Added {
                index_created: true
            }
        );
        assert!(sql_of(&db, "llm_logs")
            .unwrap()
            .ends_with(", \"storytellerRoundId\" TEXT DEFAULT NULL)"));
        assert_eq!(
            sql_of(&db, INDEX_NAME).as_deref(),
            Some(
                r#"CREATE INDEX "idx_llm_logs_storytellerRoundId" ON "llm_logs" ("storytellerRoundId")"#
            )
        );
        assert_eq!(
            ensure_llm_logs_storyteller_round_id(&db).unwrap(),
            LlmLogsRoundIdOutcome::NotNeeded
        );
    }

    /// `COLUMN_WITHOUT_INDEX_NOT_NEEDED` — the index is outside v4's gate.
    #[test]
    fn the_column_without_the_index_is_not_needed() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(&PRE_ROUND.replace(
            "  \"autonomousRunId\" TEXT,\n",
            "  \"autonomousRunId\" TEXT,\n  \"storytellerRoundId\" TEXT,\n",
        ))
        .unwrap();
        assert_eq!(
            ensure_llm_logs_storyteller_round_id(&db).unwrap(),
            LlmLogsRoundIdOutcome::NotNeeded
        );
        assert!(sql_of(&db, INDEX_NAME).is_none());
    }

    #[test]
    fn no_table_is_a_no_op() {
        let db = Connection::open_in_memory().unwrap();
        assert_eq!(
            ensure_llm_logs_storyteller_round_id(&db).unwrap(),
            LlmLogsRoundIdOutcome::NotNeeded
        );
    }
}
