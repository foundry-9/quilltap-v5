//! P4.D288 (C1-2) tier-2 differential: `db::tfidf_text_shape_repair::ensure_tfidf_text_shape_version` against v4's REAL
//! `add-tfidf-text-shape-version-v1`: the column appended `INTEGER DEFAULT 0` (R-R —
//! the dump's is `REAL DEFAULT 0`; every reader takes both), the no-op arms.
//!
//! The comparands are EFFECTS (`p4d288_ensure_support`): `sqlite_master`, each table's
//! `PRAGMA table_info` and every row through `quote()`, the ledger rows; and the ids
//! each side RAN. v4's migration lines are NO-PORT.
//!
//! Regenerate + run (the case builds the base files, one family per invocation):
//!   V5W=${V5W:-$HOME/source/quilltap-v5}
//!   N=~/.nvm/versions/node/v24.13.1/bin
//!   rm -rf /tmp/qt-p4d288-ensures/tfidf-text-shape /tmp/oracle-p4d288-ensure-tfidf-text-shape.ndjson
//!   cd ~/source/quilltap-server
//!   QT_P4D288_ENSURE_FAMILY=tfidf-text-shape QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     $N/npx tsx $V5W/harness/oracle/cases/p4d288-ensures.ts \
//!     > /tmp/oracle-p4d288-ensure-tfidf-text-shape.ndjson
//!   cd $V5W
//!   QT_ORACLE_P4D288_ENSURE_TFIDF_TEXT_SHAPE=/tmp/oracle-p4d288-ensure-tfidf-text-shape.ndjson \
//!   QT_FIXTURE_P4D288_ENSURES_DIR=/tmp/qt-p4d288-ensures \
//!     cargo test -p quilltap-harness --test tfidf_text_shape_ensure_equivalence -- --nocapture

mod p4d288_ensure_support;

#[allow(unused_imports)]
use p4d288_ensure_support::{run_family, RuledMode};

#[test]
fn tfidf_text_shape_ensure_matches_v4() {
    run_family(
        "tfidf-text-shape",
        &["tfidf_vocabularies"],
        &["pre-round", "current", "no-table"],
        &[],
        |c| {
            if quilltap_core::db::tfidf_text_shape_repair::ensure_tfidf_text_shape_version(c)
                .expect("ensure")
            {
                vec![quilltap_core::db::tfidf_text_shape_repair::MIGRATION_ID]
            } else {
                vec![]
            }
        },
    );
}
