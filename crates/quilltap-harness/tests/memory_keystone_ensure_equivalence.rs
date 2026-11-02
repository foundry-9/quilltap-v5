//! P4.D341 — the keystone boot ensure (`db::memory_keystone_repair`) against
//! v4's REAL `add-memory-keystone-v1` (v4 `8cb584eff`, the SEVENTEENTH D23
//! move) through v4's REAL `MigrationRunner` narrowed to it, in five shapes
//! (the oracle case `harness/oracle/cases/memory-keystone-ensure.ts` builds
//! the base files at the pin from v4's own generateDDL; its header names
//! them): a (column + index absent), b (no table), c (already migrated),
//! d (generateDDL-current, index absent — v4's `!hasIndex()` arm, R-4),
//! e (generateDDL-current + index).
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info`, the table's and the
//! index's `sqlite_master.sql`, every row's `quote()`d `keystone` + storage
//! class after the shared `keystone = 1` plant on `m2`, and on (a) / (d) the
//! `EXPLAIN QUERY PLAN` detail of the keystone read (the partial index USED on
//! both sides). v4's report is asserted per mode (`+RAN` on a and d only). The
//! ledger is the ruled `UNSTAMPED_COLUMN_ENSURES` asymmetry, pinned both ways:
//! v4 stamps on a / d, v5 never.
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-memory-keystone-ensure /tmp/oracle-memory-keystone-ensure.ndjson
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-memory-keystone-ensure $N/npx tsx $V5W/harness/oracle/cases/memory-keystone-ensure.ts > /tmp/oracle-memory-keystone-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_MEMORY_KEYSTONE_ENSURE=/tmp/oracle-memory-keystone-ensure.ndjson QT_FIXTURE_MEMORY_KEYSTONE_ENSURE_DIR=/tmp/qt-memory-keystone-ensure cargo test -p quilltap-harness --test memory_keystone_ensure_equivalence -- --nocapture

mod keystone_ensure_support;

use std::path::Path;

use keystone_ensure_support as kit;
use quilltap_core::db::memory_keystone_repair::{
    ensure_memory_keystone, KeystoneEnsureOutcome, INDEX_NAME, MIGRATION_ID,
};
use quilltap_core::db::Writer;
use serde_json::{json, Value};

const EQP_SQL: &str =
    "EXPLAIN QUERY PLAN SELECT id FROM memories WHERE characterId = 'k1' AND keystone = 1";

#[test]
fn the_keystone_ensure_matches_v4s_migration_in_five_shapes() {
    let (Some(oracle), Some(dir)) = (
        std::env::var_os("QT_ORACLE_MEMORY_KEYSTONE_ENSURE"),
        std::env::var_os("QT_FIXTURE_MEMORY_KEYSTONE_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_MEMORY_KEYSTONE_ENSURE and QT_FIXTURE_MEMORY_KEYSTONE_ENSURE_DIR (see header)."
        );
        return;
    };
    let mut modes = Vec::new();
    for v4 in kit::read_ndjson(&oracle) {
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = kit::report(&v4);
        let runs = matches!(mode.as_str(), "a" | "d");
        assert_eq!(report.len(), 1, "[{mode}] {report:?}");
        if runs {
            assert!(
                report[0].starts_with(&format!("+RAN {MIGRATION_ID} ")),
                "[{mode}] {report:?}"
            );
        } else {
            assert!(report[0].contains(" not needed"), "[{mode}] {report:?}");
        }
        let want_ledger: Vec<String> = if runs {
            vec![MIGRATION_ID.into()]
        } else {
            vec![]
        };
        assert_eq!(
            serde_json::from_value::<Vec<String>>(v4["ledger"].clone()).unwrap(),
            want_ledger,
            "[{mode}] v4's ledger (UNSTAMPED_COLUMN_ENSURES, v4's side)"
        );

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(Path::new(&dir).join(format!("keystone-{mode}.db")), &work).unwrap();
        let w = Writer::open_writable(&work, kit::PEPPER).unwrap();
        let c = w.connection();
        let before = kit::table_sql(c, "memories");
        let outcome = ensure_memory_keystone(c).expect("ensure");
        let want = match mode.as_str() {
            "a" => KeystoneEnsureOutcome::Changed {
                column_added: true,
                index_created: true,
            },
            "d" => KeystoneEnsureOutcome::Changed {
                column_added: false,
                index_created: true,
            },
            _ => KeystoneEnsureOutcome::NotNeeded,
        };
        assert_eq!(outcome, want, "[{mode}] the ensure's answer");
        if mode != "a" {
            assert_eq!(
                kit::table_sql(c, "memories"),
                before,
                "[{mode}] table touched"
            );
        }
        if kit::has_column(c, "memories", "keystone") {
            c.execute("UPDATE memories SET keystone = 1 WHERE id = 'm2'", [])
                .unwrap();
        }
        let eqp: Value = if runs {
            let mut stmt = c.prepare(EQP_SQL).unwrap();
            let details: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(3))
                .unwrap()
                .map(Result::unwrap)
                .collect();
            json!(details)
        } else {
            Value::Null
        };
        kit::assert_bytes(
            &kit::table_info(c, "memories"),
            &v4["tableInfo"],
            &format!("[{mode}] table_info"),
        );
        kit::assert_bytes(
            &json!(kit::table_sql(c, "memories")),
            &v4["sql"],
            &format!("[{mode}] table sql"),
        );
        kit::assert_bytes(
            &json!(kit::index_sql(c, INDEX_NAME)),
            &v4["indexSql"],
            &format!("[{mode}] index sql"),
        );
        kit::assert_bytes(
            &kit::cells(c, "memories", &["keystone"]),
            &v4["rows"],
            &format!("[{mode}] cells"),
        );
        kit::assert_bytes(&eqp, &v4["eqp"], &format!("[{mode}] EXPLAIN QUERY PLAN"));
        assert!(
            kit::ledger(c).is_empty(),
            "[{mode}] v5 stamped (UNSTAMPED_COLUMN_ENSURES)"
        );
        if runs {
            assert!(
                eqp[0].as_str().unwrap().contains(INDEX_NAME),
                "[{mode}] the partial index is USED: {eqp}"
            );
        }
        if mode == "a" {
            let sql = v4["sql"].as_str().unwrap();
            assert!(
                sql.ends_with(", \"keystone\" INTEGER NOT NULL DEFAULT 0)"),
                "marker {sql}"
            );
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d", "e"], "every shape ran");
    eprintln!("OK: the keystone ensure matched v4 in shapes {modes:?}.");
}
