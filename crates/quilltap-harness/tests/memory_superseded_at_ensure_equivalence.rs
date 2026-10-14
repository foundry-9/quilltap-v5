//! P4.D288 (C1-2) tier-2 differential: `db::memory_superseded_at_repair::ensure_memory_superseded_at` against v4's REAL
//! `add-memory-superseded-at-v1` (bug 225): the column appended in the MIGRATION's
//! text, the backfill from `updatedAt` for superseded rows only, a pre-tiers table
//! gaining the column with no backfill, the no-op arms.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/superseded-at /tmp/oracle-p4d288-ensure-superseded-at.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=superseded-at QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-superseded-at.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_SUPERSEDED_AT=/tmp/oracle-p4d288-ensure-superseded-at.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test memory_superseded_at_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn memory_superseded_at_ensure_matches_v4() {
    run_family(
        "superseded-at",
        &["memories"],
        &[
            "pre-round",
            "no-table",
            "column-present-unstamped",
            "already-migrated",
            "pre-tiers",
        ],
        &[],
        |c| {
            let o = quilltap_core::db::memory_superseded_at_repair::ensure_memory_superseded_at(c)
                .expect("ensure");
            if o.is_noop() {
                vec![]
            } else {
                vec![quilltap_core::db::memory_superseded_at_repair::MIGRATION_ID]
            }
        },
    );
}
