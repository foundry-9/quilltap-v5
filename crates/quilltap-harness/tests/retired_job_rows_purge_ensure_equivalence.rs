//! P4.D288 (C1-2) tier-2 differential: `db::retired_job_rows_purge::{purge_inter_character_memory_jobs,
//! purge_housekeeping_consolidation_jobs}` (in v4's slot order) against v4's REAL
//! `drop-inter-character-memory-jobs-v1` → `drop-housekeeping-consolidation-jobs-v1`:
//! every row of the retired types deleted whatever its status, the other types kept;
//! the no-op arms.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/retired-jobs /tmp/oracle-p4d288-ensure-retired-jobs.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=retired-jobs QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-retired-jobs.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_RETIRED_JOBS=/tmp/oracle-p4d288-ensure-retired-jobs.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test retired_job_rows_purge_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn retired_job_rows_purge_ensure_matches_v4() {
    run_family(
        "retired-jobs",
        &["background_jobs"],
        &["mixed", "none-retired", "no-table"],
        &[],
        |c| {
            let mut ran = Vec::new();
            if quilltap_core::db::retired_job_rows_purge::purge_inter_character_memory_jobs(c)
                .expect("inter")
                > 0
            {
                ran.push(quilltap_core::db::retired_job_rows_purge::INTER_CHARACTER_MIGRATION_ID);
            }
            if quilltap_core::db::retired_job_rows_purge::purge_housekeeping_consolidation_jobs(c)
                .expect("hk")
                > 0
            {
                ran.push(quilltap_core::db::retired_job_rows_purge::HOUSEKEEPING_CONSOLIDATION_MIGRATION_ID);
            }
            ran
        },
    );
}
