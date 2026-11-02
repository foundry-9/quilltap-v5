//! P4.D341 — the Storyteller-mode boot ensure (`db::storyteller_mode_repair`)
//! against v4's REAL `add-storyteller-mode-v1` (v4 `b15fe9404`, the
//! EIGHTEENTH D23 move) through v4's REAL `MigrationRunner` narrowed to it, in
//! six shapes (`harness/oracle/cases/storyteller-mode-ensure.ts` builds the
//! bases at the pin from v4's REAL repositories' generateDDL; its header names
//! them): a (three columns + table absent), b (no `chats`), c (migrated),
//! d (generateDDL-current, a fresh provision), e (only `storytellerBid`
//! missing), f (only the table missing).
//!
//! Comparands per shape, byte-exact: `PRAGMA table_info` of `chats` /
//! `chat_messages` / `storyteller_rounds`, `sqlite_master.sql` of the three
//! tables and both indexes (on a / f the table text is the MIGRATION's DDL —
//! R-2's FK line asserted — BYTE-EQUAL both sides), every chat's `turnMode` /
//! `storytellerInviteFilter`, every message's `storytellerBid` and every
//! round's cells as `quote()` + `typeof` (on a / d after v4's REAL
//! `StorytellerRoundsRepository.create` wrote one round and v5 wrote the same
//! cells raw — the storage-class leg; v5's repository is P4.D343's).
//! `CREATED_AT_INDEX_ON_FIRST_TOUCH`: v4's case touches its repository once on
//! every mode with `chats` (its `ensureCollection` makes generateDDL's
//! createdAt index on first access); v5's ensure makes the same index at boot
//! — on (c) through the named `HALF_MADE_TABLE_INDEXES` arm. The ledger is
//! `UNSTAMPED_COLUMN_ENSURES`, both ways (v4 stamps a / e / f).
//!
//! Generate (Node 24, from the pinned v4 worktree):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   rm -rf /tmp/qt-storyteller-mode-ensure /tmp/oracle-storyteller-mode-ensure.ndjson
//!   QT_FIXTURE_OUT_DIR=/tmp/qt-storyteller-mode-ensure $N/npx tsx $V5W/harness/oracle/cases/storyteller-mode-ensure.ts > /tmp/oracle-storyteller-mode-ensure.ndjson
//! Run:
//!   cd $V5W
//!   QT_ORACLE_STORYTELLER_MODE_ENSURE=/tmp/oracle-storyteller-mode-ensure.ndjson QT_FIXTURE_STORYTELLER_MODE_ENSURE_DIR=/tmp/qt-storyteller-mode-ensure cargo test -p quilltap-harness --test storyteller_mode_ensure_equivalence -- --nocapture

mod keystone_ensure_support;

use std::path::Path;

use keystone_ensure_support as kit;
use quilltap_core::db::storyteller_mode_repair::{
    run_storyteller_mode_ensures, StorytellerEnsureOutcome, CHAT_INDEX, CREATED_AT_INDEX,
    MIGRATION_ID, STORYTELLER_ROUNDS_TABLE,
};
use quilltap_core::db::Writer;
use rusqlite::types::Value as Sql;
use serde_json::{json, Value};

const ROUND_COLUMNS: [&str; 9] = [
    "chatId",
    "triggerMessageId",
    "status",
    "bids",
    "selection",
    "closedReason",
    "createdAt",
    "updatedAt",
    "closedAt",
];

fn sql_value(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::String(s) => Sql::Text(s.clone()),
        Value::Number(n) if n.is_i64() => Sql::Integer(n.as_i64().unwrap()),
        Value::Number(n) => Sql::Real(n.as_f64().unwrap()),
        other => panic!("unexpected cell {other}"),
    }
}

#[test]
fn the_storyteller_ensure_matches_v4s_migration_in_six_shapes() {
    let (Some(oracle), Some(dir)) = (
        std::env::var_os("QT_ORACLE_STORYTELLER_MODE_ENSURE"),
        std::env::var_os("QT_FIXTURE_STORYTELLER_MODE_ENSURE_DIR"),
    ) else {
        eprintln!(
            "SKIP: set QT_ORACLE_STORYTELLER_MODE_ENSURE and QT_FIXTURE_STORYTELLER_MODE_ENSURE_DIR (see header)."
        );
        return;
    };
    let mut modes = Vec::new();
    for v4 in kit::read_ndjson(&oracle) {
        let mode = v4["mode"].as_str().unwrap().to_string();
        let report = kit::report(&v4);
        let runs = matches!(mode.as_str(), "a" | "e" | "f");
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
            "[{mode}] v4's ledger"
        );

        let scratch = tempfile::tempdir().unwrap();
        let work = scratch.path().join("work.db");
        std::fs::copy(
            Path::new(&dir).join(format!("storyteller-{mode}.db")),
            &work,
        )
        .unwrap();
        let w = Writer::open_writable(&work, kit::PEPPER).unwrap();
        let c = w.connection();
        let outcome = run_storyteller_mode_ensures(c).expect("ensure");
        let want = match mode.as_str() {
            "a" => StorytellerEnsureOutcome {
                columns_added: 3,
                table_created: true,
                indexes_healed: 0,
            },
            "e" => StorytellerEnsureOutcome {
                columns_added: 1,
                table_created: false,
                indexes_healed: 0,
            },
            "f" => StorytellerEnsureOutcome {
                columns_added: 0,
                table_created: true,
                indexes_healed: 0,
            },
            // (c): v4's migration made the table + chat index only; the
            // createdAt index comes from the repository's first touch.
            "c" => StorytellerEnsureOutcome {
                columns_added: 0,
                table_created: false,
                indexes_healed: 1,
            },
            _ => StorytellerEnsureOutcome::default(),
        };
        assert_eq!(outcome, want, "[{mode}] the ensure's answer");
        if !v4["wrote"].is_null() {
            let cols: Vec<String> = std::iter::once("id".to_string())
                .chain(ROUND_COLUMNS.iter().map(|s| s.to_string()))
                .collect();
            let values: Vec<Sql> = cols.iter().map(|k| sql_value(&v4["wrote"][k])).collect();
            c.execute(
                &format!(
                    "INSERT INTO storyteller_rounds ({}) VALUES ({})",
                    cols.iter()
                        .map(|k| format!("\"{k}\""))
                        .collect::<Vec<_>>()
                        .join(", "),
                    vec!["?"; cols.len()].join(", ")
                ),
                rusqlite::params_from_iter(values),
            )
            .unwrap();
        }
        let got = json!({
            "tableInfo": {
                "chats": kit::table_info(c, "chats"),
                "chat_messages": kit::table_info(c, "chat_messages"),
                "storyteller_rounds": kit::table_info(c, STORYTELLER_ROUNDS_TABLE),
            },
            "sql": {
                "chats": kit::table_sql(c, "chats"),
                "chat_messages": kit::table_sql(c, "chat_messages"),
                "storyteller_rounds": kit::table_sql(c, STORYTELLER_ROUNDS_TABLE),
                "idx_storyteller_rounds_chat": kit::index_sql(c, CHAT_INDEX),
                "idx_storyteller_rounds_createdAt": kit::index_sql(c, CREATED_AT_INDEX),
            },
            "rows": {
                "chats": kit::cells(c, "chats", &["turnMode", "storytellerInviteFilter"]),
                "chat_messages": kit::cells(c, "chat_messages", &["storytellerBid"]),
                "storyteller_rounds": kit::cells(c, STORYTELLER_ROUNDS_TABLE, &ROUND_COLUMNS),
            },
        });
        for key in ["tableInfo", "sql", "rows"] {
            kit::assert_bytes(&got[key], &v4[key], &format!("[{mode}] {key}"));
        }
        assert!(kit::ledger(c).is_empty(), "[{mode}] v5 stamped");
        if matches!(mode.as_str(), "a" | "f") {
            let table = v4["sql"]["storyteller_rounds"].as_str().unwrap();
            assert!(
                table.contains(
                    "FOREIGN KEY (\"chatId\") REFERENCES \"chats\"(\"id\") ON DELETE CASCADE"
                ),
                "[{mode}] R-2's FK marker: {table}"
            );
        }
        if mode == "a" {
            assert!(
                v4["sql"]["chats"].as_str().unwrap().ends_with(
                    ", \"turnMode\" TEXT DEFAULT NULL, \"storytellerInviteFilter\" INTEGER DEFAULT 0)"
                ),
                "marker"
            );
        }
        modes.push(mode);
    }
    assert_eq!(modes, ["a", "b", "c", "d", "e", "f"], "every shape ran");
    eprintln!("OK: the Storyteller ensure matched v4 in shapes {modes:?}.");
}
