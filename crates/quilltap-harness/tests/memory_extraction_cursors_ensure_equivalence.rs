//! P4.D288 (C1-2) tier-2 differential: the CHAIN `db::memory_episode_watermark_repair` → `db::memory_extraction_cursors_repair`
//! (R-D, v4's registration order) against v4's REAL `add-episode-extraction-watermark-v1`
//! → `add-memory-extraction-cursors-v1` run through v4's REAL `MigrationRunner` narrowed
//! to the two (the ledger consulted and STAMPED, as on a v4 boot): the episode column
//! added and backfilled from each chat's last summary anchor, the cursor table in the
//! MIGRATION text, both legacy columns copied (`<chatId>:<pass>`, NULL / `''` skipped,
//! a conflict kept) and DROPPED, and BOTH ledger rows with v4's message bytes — over
//! pre-round, already-migrated, half-migrated (a crashed v4 boot), mid-chain, no-chats.
//!
//! **`FRESH_INSTANCE_UNLEDGERED`** (a named both-ways row): a v5-provisioned instance
//! carries the cursor table and no `migrations_state`, so v4's runner (empty ledger)
//! re-runs BOTH migrations on it — re-adding the episode column, inserting
//! `episode-fold` cursors at the last summary anchor, dropping the column — while v5's
//! O-3 gate (the cursor table present) does nothing. A fresh v5 instance carries no v4
//! ledger at all (P4.153's `ORACLE_ONLY_TABLES` row), so every v4 migration whose
//! `shouldRun` holds re-runs there; this row records the chain's share. Pinned both
//! ways: v4's report as measured, v5's two ENSURES running nothing. At BOOT the
//! hazard is closed by P4.D288 unit 13: the span chain's
//! `memory_extraction_cursors_repair::stamp_settled_chain` records v4's two rows
//! on exactly this state, and `zero_migrations_rerun_equivalence` pins that v4
//! then re-runs nothing (`fresh` / `fresh-used`) and that the rows are v4's bytes.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/cursors-chain /tmp/oracle-p4d288-ensure-cursors-chain.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=cursors-chain QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-cursors-chain.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_CURSORS_CHAIN=/tmp/oracle-p4d288-ensure-cursors-chain.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test memory_extraction_cursors_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn memory_extraction_cursors_ensure_matches_v4() {
    run_family(
        "cursors-chain",
        &["chats", "memory_extraction_cursors"],
        &["pre-round", "already-migrated", "half-migrated", "mid-chain", "no-chats", "fresh-v5-unledgered"],
        &[RuledMode {
            name: "FRESH_INSTANCE_UNLEDGERED",
            mode: "fresh-v5-unledgered",
            v4_report: &[
                "+RAN add-episode-extraction-watermark-v1 (Episode watermark in place (1 chats backfilled))",
                "+RAN add-memory-extraction-cursors-v1 (Extraction cursors in place (1 carried over from 1 watermark column(s)))",
            ],
            v5_ran: &[],
        }],
        |c| {
        let mut ran = Vec::new();
        if quilltap_core::db::memory_episode_watermark_repair::ensure_memory_episode_watermark(c)
            .expect("episode")
            .is_some()
        {
            ran.push(quilltap_core::db::memory_episode_watermark_repair::MIGRATION_ID);
        }
        if quilltap_core::db::memory_extraction_cursors_repair::ensure_memory_extraction_cursors(c)
            .expect("cursors")
            .is_some()
        {
            ran.push(quilltap_core::db::memory_extraction_cursors_repair::MIGRATION_ID);
        }
        ran
    },
    );
}
