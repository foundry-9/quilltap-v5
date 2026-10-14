//! P4.D288 (C1-2) tier-2 differential: `db::memories_superseded_by_index::ensure_memories_superseded_by_index` against v4's
//! REAL `add-memories-superseded-by-index-v1`: the partial index created (its
//! `sqlite_master.sql` byte-equal), the no-op arms. A second test holds the stored
//! text equal to `migration_indexes.json`'s statement, so P4.160's later replay finds
//! the SAME index from both routes.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/superseded-by-index /tmp/oracle-p4d288-ensure-superseded-by-index.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=superseded-by-index QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-superseded-by-index.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_SUPERSEDED_BY_INDEX=/tmp/oracle-p4d288-ensure-superseded-by-index.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test memories_superseded_by_index_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn memories_superseded_by_index_ensure_matches_v4() {
    run_family(
        "superseded-by-index",
        &["memories"],
        &["index-absent", "pre-tiers", "index-present", "no-table"],
        &[],
        |c| {
            if quilltap_core::db::memories_superseded_by_index::ensure_memories_superseded_by_index(
                c,
            )
            .expect("ensure")
            {
                vec![quilltap_core::db::memories_superseded_by_index::MIGRATION_ID]
            } else {
                vec![]
            }
        },
    );
}

/// The ensure's stored `sqlite_master.sql` is byte-equal to the statement
/// `migration_indexes.json` carries, so P4.160's backfill (which runs later on
/// the same boot) finds the SAME index whichever route made it.
#[test]
fn the_ensure_and_the_index_family_store_the_same_text() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let fresh: serde_json::Value = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/provisioning/fresh_schema.json"
    ))
    .unwrap();
    let memories = fresh["main"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .find(|s| s.starts_with("CREATE TABLE \"memories\" ("))
        .unwrap();
    conn.execute_batch(memories).unwrap();
    assert!(
        quilltap_core::db::memories_superseded_by_index::ensure_memories_superseded_by_index(&conn)
            .unwrap()
    );
    let stored: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name = 'idx_memories_supersededById'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let indexes: serde_json::Value = serde_json::from_str(include_str!(
        "../../quilltap-core/src/services/provisioning/migration_indexes.json"
    ))
    .unwrap();
    let family = indexes["main"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .find(|s| s.starts_with("CREATE INDEX \"idx_memories_supersededById\""))
        .expect("migration_indexes.json carries the index");
    assert_eq!(stored, family);
    eprintln!("OK: the ensure and the index family store one text.");
}
