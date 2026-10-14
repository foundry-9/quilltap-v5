//! P4.D288 (C1-2) tier-2 differential: `db::memory_vector_mirror_retire::retire_memory_vector_mirror` against v4's REAL
//! `retire-memory-vector-entries-v1`: > 500 mirror rows across two pages, a BLOB
//! copied byte-equal, a JSON-text vector (array and index-keyed object) converted to
//! the QUANTIZED blob byte-equal to v4's `float32ToBlob`, an existing embedding (an
//! EMPTY blob included) kept, an orphan discarded, unusable mirrors (empty blob,
//! `null`, `[]`, a number) skipped, then both tables dropped; the no-op arms.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/vector-retire /tmp/oracle-p4d288-ensure-vector-retire.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=vector-retire QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-vector-retire.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_VECTOR_RETIRE=/tmp/oracle-p4d288-ensure-vector-retire.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test memory_vector_mirror_retire_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn memory_vector_mirror_retire_ensure_matches_v4() {
    run_family(
        "vector-retire",
        &["memories", "vector_entries", "vector_indices"],
        &["mirror-two-pages", "indices-only", "neither", "no-memories"],
        &[],
        |c| {
            if quilltap_core::db::memory_vector_mirror_retire::retire_memory_vector_mirror(c)
                .expect("retire")
                .is_some()
            {
                vec![quilltap_core::db::memory_vector_mirror_retire::MIGRATION_ID]
            } else {
                vec![]
            }
        },
    );
}
