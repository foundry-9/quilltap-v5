//! P4.D288 (C1-2) tier-2 differential: `db::chats_scenario_source_repair::ensure_chats_scenario_source` against v4's REAL
//! `add-chat-scenario-source-v1`: both columns appended `TEXT DEFAULT NULL`, the
//! half-migrated table gaining only the missing one, the no-op arms.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/scenario-source /tmp/oracle-p4d288-ensure-scenario-source.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=scenario-source QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-scenario-source.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_SCENARIO_SOURCE=/tmp/oracle-p4d288-ensure-scenario-source.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test chats_scenario_source_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn chats_scenario_source_ensure_matches_v4() {
    run_family(
        "scenario-source",
        &["chats"],
        &["pre-round", "no-table", "half", "current"],
        &[],
        |c| {
            let added =
                quilltap_core::db::chats_scenario_source_repair::ensure_chats_scenario_source(c)
                    .expect("ensure");
            if added.is_empty() {
                vec![]
            } else {
                vec![quilltap_core::db::chats_scenario_source_repair::MIGRATION_ID]
            }
        },
    );
}
