//! P4.D288 (C1-2) tier-2 differential: `db::memory_maintenance_settings_move::ensure_memory_maintenance_settings_move`
//! against v4's REAL `move-memory-maintenance-settings-v1`: the STORED
//! `memoryMaintenance` bytes byte-compared (the clamp, V8's own-key order of the
//! overrides, no `phases`, the merge keys dropped), the single user's row winning
//! (even NULL), the latest-`updatedAt` fallback, an existing key kept, the dead key
//! deleted, the three columns dropped, the no-op arms.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/maintenance-move /tmp/oracle-p4d288-ensure-maintenance-move.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=maintenance-move QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-maintenance-move.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_MAINTENANCE_MOVE=/tmp/oracle-p4d288-ensure-maintenance-move.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test memory_maintenance_settings_move_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn memory_maintenance_settings_move_ensure_matches_v4() {
    run_family(
        "maintenance-move",
        &["chat_settings", "instance_settings"],
        &[
            "single-user",
            "latest-row",
            "single-user-null",
            "key-exists",
            "dead-key-only",
            "garbage-value",
            "no-instance-settings",
            "nothing-dead",
        ],
        &[],
        |c| {
            let o = quilltap_core::db::memory_maintenance_settings_move::ensure_memory_maintenance_settings_move(c)
            .expect("ensure");
            if o.ran {
                vec![quilltap_core::db::memory_maintenance_settings_move::MIGRATION_ID]
            } else {
                vec![]
            }
        },
    );
}
